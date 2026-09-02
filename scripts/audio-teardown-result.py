#!/usr/bin/env python3
"""Authenticate one B7 guest playback, shutdown, and typed CoreAudio teardown log."""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from pathlib import Path

SCHEMA = "bridgevm.b7-audio-teardown-lane.v1"
PREFIX = "hda CoreAudio stats:"
STAT_FIELDS = (
    "frames_rendered", "drops", "dropped_bytes", "format_drops", "ring_full_drops",
    "queue_stop_errors", "queue_dispose_errors",
    "callback_errors", "callback_active_errors", "callback_stopping_errors",
    "callback_expected_stopping_errors", "callback_unexpected_errors",
    "callback_stopping_invalid_run_state", "callback_stopping_queue_invalidated",
    "callback_stopping_enqueue_during_reset", "callback_stopping_disposal_pending",
    "callback_stopping_unclassified",
)
STATUS_REASON = {
    -66_678: "stopping-invalid-run-state",
    -66_671: "stopping-queue-invalidated",
    -66_632: "stopping-enqueue-during-reset",
    -66_685: "stopping-disposal-pending",
}
EXPECTED_STOPPING = {-66_678, -66_632, -66_685}
EVENT = re.compile(
    r"^hda CoreAudio callback enqueue: state=(active|stopping) "
    r"reason=([a-z-]+) osstatus=(-?[0-9]+) expected=(true|false)$"
)
LIFECYCLE = re.compile(r"^hda CoreAudio lifecycle: operation=(stop|dispose) osstatus=(-?[0-9]+) success=(true|false)$")
NONCE = re.compile(r"^[0-9a-f]{64}$")


class AudioTeardownError(ValueError):
    pass


def seal(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def parse_stats(line: str) -> dict[str, int]:
    if not line.startswith(PREFIX + " "):
        raise AudioTeardownError("CoreAudio stats prefix is missing")
    values: dict[str, int] = {}
    for token in line[len(PREFIX) + 1:].split(" "):
        fields = token.split("=", 1)
        if len(fields) != 2 or fields[0] in values or not re.fullmatch(r"[0-9]+", fields[1]):
            raise AudioTeardownError("CoreAudio stats contain a malformed or duplicate token")
        values[fields[0]] = int(fields[1])
    if tuple(values) != STAT_FIELDS:
        raise AudioTeardownError("CoreAudio stats do not have the exact ordered field set")
    if values["drops"] != values["format_drops"] + values["ring_full_drops"]:
        raise AudioTeardownError("drop counters do not reconcile")
    if values["callback_errors"] != values["callback_active_errors"] + values["callback_stopping_errors"]:
        raise AudioTeardownError("callback total does not reconcile with active and stopping")
    expected_typed = sum(values[key] for key in (
        "callback_stopping_invalid_run_state", "callback_stopping_enqueue_during_reset",
        "callback_stopping_disposal_pending",
    ))
    typed = expected_typed + values["callback_stopping_queue_invalidated"]
    if values["callback_expected_stopping_errors"] != expected_typed:
        raise AudioTeardownError("expected stopping total does not reconcile with typed reasons")
    if values["callback_stopping_errors"] != typed + values["callback_stopping_unclassified"]:
        raise AudioTeardownError("stopping total does not reconcile")
    if values["callback_unexpected_errors"] != (
        values["callback_active_errors"] + values["callback_stopping_queue_invalidated"]
        + values["callback_stopping_unclassified"]
    ):
        raise AudioTeardownError("unexpected total does not reconcile")
    return values


def parse_events(lines: list[str], stats: dict[str, int]) -> None:
    reason_fields = (
        "callback_stopping_invalid_run_state", "callback_stopping_queue_invalidated",
        "callback_stopping_enqueue_during_reset", "callback_stopping_disposal_pending",
        "callback_stopping_unclassified",
    )
    counts = {key: 0 for key in reason_fields}
    active = 0
    events = 0
    lifecycle: dict[str, int] = {}
    for line in lines:
        if line.startswith("hda CoreAudio lifecycle:"):
            match = LIFECYCLE.fullmatch(line)
            if match is None or match[1] in lifecycle or ((int(match[2]) == 0) != (match[3] == "true")):
                raise AudioTeardownError("CoreAudio lifecycle event is malformed or duplicated")
            lifecycle[match[1]] = int(match[2])
            continue
        if not line.startswith("hda CoreAudio callback enqueue:"):
            continue
        match = EVENT.fullmatch(line)
        if match is None:
            raise AudioTeardownError("callback event is malformed")
        state, reason, raw_status, raw_expected = match.groups()
        status = int(raw_status)
        expected = raw_expected == "true"
        events += 1
        if state == "active":
            if reason != "active-enqueue-failure" or expected:
                raise AudioTeardownError("active callback event has false classification")
            active += 1
            continue
        classified = STATUS_REASON.get(status, "stopping-unclassified")
        if reason != classified or expected != (status in EXPECTED_STOPPING):
            raise AudioTeardownError("stopping callback event has false OSStatus classification")
        field = "callback_" + classified.replace("-", "_")
        counts[field] += 1
    if events != stats["callback_errors"] or active != stats["callback_active_errors"]:
        raise AudioTeardownError("callback events do not authenticate their totals")
    for field, count in counts.items():
        if count != stats[field]:
            raise AudioTeardownError(f"callback events do not authenticate {field}")
    if set(lifecycle) != {"stop", "dispose"}:
        raise AudioTeardownError("exactly one stop and dispose lifecycle event is required")
    for operation, status in lifecycle.items():
        if stats[f"queue_{operation}_errors"] != int(status != 0):
            raise AudioTeardownError(f"{operation} lifecycle status does not reconcile")


def validate(run_log: Path, result_file: Path, launcher_exit: int, nonce: str, ordinal: int) -> dict:
    if not NONCE.fullmatch(nonce) or not 1 <= ordinal <= 10:
        raise AudioTeardownError("lane identity is invalid")
    if not run_log.is_file() or run_log.is_symlink() or not 1 <= run_log.stat().st_size <= 64 * 1024 * 1024:
        raise AudioTeardownError("run log is missing, unsafe, empty, or over 64 MiB")
    if launcher_exit != 0:
        raise AudioTeardownError(f"launcher exited with status {launcher_exit}")
    if not result_file.is_file() or result_file.is_symlink() or not 1 <= result_file.stat().st_size <= 4096:
        raise AudioTeardownError("playback result file is missing, unsafe, empty, or over 4 KiB")
    try:
        lines = run_log.read_text(encoding="utf-8").replace("\r\n", "\n").splitlines()
    except UnicodeError as error:
        raise AudioTeardownError("run log is not UTF-8") from error
    stats_lines = [line for line in lines if line.startswith(PREFIX)]
    if len(stats_lines) != 1:
        raise AudioTeardownError("run log must contain exactly one final CoreAudio stats line")
    stats = parse_stats(stats_lines[0])
    parse_events(lines, stats)
    marker = f"B7 PLAYBACK PASS nonce={nonce} wav_bytes=384044"
    try:
        result_lines = result_file.read_text(encoding="utf-8").replace("\r\n", "\n").splitlines()
    except UnicodeError as error:
        raise AudioTeardownError("playback result file is not UTF-8") from error
    if result_lines != [marker]:
        raise AudioTeardownError("nonce-bound playback result is not the exact marker")
    if sum(bool(re.fullmatch(r"stop: PSCI .*\(system off\)", line)) for line in lines) != 1:
        raise AudioTeardownError("exactly one clean guest SYSTEM_OFF is required")
    if sum(line.startswith("NVMe disk written back:") for line in lines) != 1:
        raise AudioTeardownError("exactly one NVMe writeback record is required")
    if (stats["frames_rendered"] <= 0 or stats["drops"] != 0
            or stats["queue_stop_errors"] != 0 or stats["queue_dispose_errors"] != 0
            or stats["callback_unexpected_errors"] != 0):
        raise AudioTeardownError("lane lacks rendered, drop-free playback and clean teardown")
    return {
        "schema_version": SCHEMA,
        "ordinal": ordinal,
        "nonce": nonce,
        "run_log_sha256": seal(run_log),
        "playback_result_sha256": seal(result_file),
        **stats,
        "playback_pass": True,
        "shutdown_pass": True,
        "pass": True,
    }


def self_test() -> int:
    import tempfile

    nonce = "a" * 64
    base = "hda CoreAudio lifecycle: operation=stop osstatus=0 success=true\n" \
        "hda CoreAudio lifecycle: operation=dispose osstatus=0 success=true\n" \
        "stop: PSCI SYSTEM_OFF (system off)\nNVMe disk written back: fixture\n"
    zero = (
        f"{PREFIX} frames_rendered=96000 drops=0 dropped_bytes=0 format_drops=0 "
        "ring_full_drops=0 queue_stop_errors=0 queue_dispose_errors=0 "
        "callback_errors=0 callback_active_errors=0 "
        "callback_stopping_errors=0 callback_expected_stopping_errors=0 "
        "callback_unexpected_errors=0 callback_stopping_invalid_run_state=0 "
        "callback_stopping_queue_invalidated=0 callback_stopping_enqueue_during_reset=0 "
        "callback_stopping_disposal_pending=0 callback_stopping_unclassified=0\n"
    )
    expected_event = "hda CoreAudio callback enqueue: state=stopping reason=stopping-enqueue-during-reset osstatus=-66632 expected=true\n"
    expected = zero.replace("callback_errors=0", "callback_errors=1").replace(
        "callback_stopping_errors=0", "callback_stopping_errors=1").replace(
        "callback_expected_stopping_errors=0", "callback_expected_stopping_errors=1").replace(
        "callback_stopping_enqueue_during_reset=0", "callback_stopping_enqueue_during_reset=1")
    with tempfile.TemporaryDirectory(prefix="bridgevm-b7-result-") as temporary:
        log = Path(temporary) / "run.log"
        result = Path(temporary) / "playback-result.txt"
        result.write_text(f"B7 PLAYBACK PASS nonce={nonce} wav_bytes=384044\n", encoding="utf-8")
        log.write_text(base + zero, encoding="utf-8")
        assert validate(log, result, 0, nonce, 1)["pass"] is True
        log.write_text(base + expected_event + expected, encoding="utf-8")
        assert validate(log, result, 0, nonce, 1)["callback_expected_stopping_errors"] == 1
        rejected = [
            base + expected,
            base + zero.replace("callback_active_errors=0", "callback_active_errors=1"),
            base.replace("stop: PSCI SYSTEM_OFF (system off)\n", "") + zero,
            base + zero + zero,
            base + expected_event.replace("expected=true", "expected=false") + expected,
            base.replace("operation=stop osstatus=0 success=true", "operation=stop osstatus=-50 success=false")
            + zero.replace("queue_stop_errors=0", "queue_stop_errors=1"),
        ]
        invalidated_event = (
            "hda CoreAudio callback enqueue: state=stopping "
            "reason=stopping-queue-invalidated osstatus=-66671 expected=false\n"
        )
        invalidated = zero.replace("callback_errors=0", "callback_errors=1").replace(
            "callback_stopping_errors=0", "callback_stopping_errors=1").replace(
            "callback_unexpected_errors=0", "callback_unexpected_errors=1").replace(
            "callback_stopping_queue_invalidated=0", "callback_stopping_queue_invalidated=1")
        rejected.append(base + invalidated_event + invalidated)
        for body in rejected:
            log.write_text(body, encoding="utf-8")
            try:
                validate(log, result, 0, nonce, 1)
            except AudioTeardownError:
                continue
            raise AssertionError("invalid B7 lane log accepted")
        result.write_text("wrong\n", encoding="utf-8")
        try: validate(log, result, 0, nonce, 1)
        except AudioTeardownError: pass
        else: raise AssertionError("invalid B7 playback result accepted")
    print(f"PASS: B7 audio teardown lane classifier ({len(rejected) + 3} cases)")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--run-log", type=Path)
    parser.add_argument("--result-file", type=Path)
    parser.add_argument("--launcher-exit", type=int)
    parser.add_argument("--nonce")
    parser.add_argument("--ordinal", type=int)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        return self_test()
    if None in (args.run_log, args.result_file, args.launcher_exit, args.nonce, args.ordinal, args.output):
        parser.error("all lane arguments are required")
    try:
        result = validate(args.run_log, args.result_file, args.launcher_exit, args.nonce, args.ordinal)
        with args.output.open("x", encoding="utf-8") as output:
            json.dump(result, output, indent=2, sort_keys=True)
            output.write("\n")
    except (OSError, AudioTeardownError) as error:
        print(f"FAIL: B7 audio teardown lane: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
