#!/usr/bin/env python3
"""Validate the final A5 CoreAudio counters without starting a VM."""

from __future__ import annotations

import argparse
import re
import sys

PREFIX = "hda CoreAudio stats:"
FIELDS = ("frames_rendered", "drops", "callback_errors")
TOKEN = re.compile(r"(?:^|\s)(frames_rendered|drops|callback_errors)=([^\s]+)")


class AudioResultError(ValueError):
    """The launcher or final telemetry cannot support an A5 pass."""


def parse_stats(line: str) -> tuple[int, int, int]:
    line = line.strip()
    if not line.startswith(PREFIX):
        raise AudioResultError("missing final 'hda CoreAudio stats:' line")

    values: dict[str, int] = {}
    for key, raw in TOKEN.findall(line):
        if key in values:
            raise AudioResultError(f"duplicate {key} field")
        if re.fullmatch(r"[0-9]+", raw) is None:
            raise AudioResultError(f"malformed {key} value: {raw!r}")
        values[key] = int(raw)

    missing = [key for key in FIELDS if key not in values]
    if missing:
        raise AudioResultError(f"missing counter field(s): {', '.join(missing)}")
    return tuple(values[key] for key in FIELDS)  # type: ignore[return-value]


def validate_result(launcher_exit: int, stats_line: str) -> tuple[int, int, int]:
    if launcher_exit != 0:
        raise AudioResultError(f"launcher exited with status {launcher_exit}")
    frames, drops, callback_errors = parse_stats(stats_line)
    if frames == 0:
        raise AudioResultError("frames_rendered must be greater than zero")
    if drops != 0:
        raise AudioResultError(f"drops must be zero, got {drops}")
    if callback_errors != 0:
        raise AudioResultError(
            f"callback_errors must be zero, got {callback_errors}"
        )
    return frames, drops, callback_errors


def self_test() -> None:
    good = (
        f"{PREFIX} frames_rendered=96000 drops=0 dropped_bytes=0 "
        "format_drops=0 ring_full_drops=0 callback_errors=0"
    )
    assert validate_result(0, good) == (96000, 0, 0)
    rejected = [
        (7, good),
        (0, good.replace("frames_rendered=96000", "frames_rendered=0")),
        (0, good.replace("drops=0", "drops=2", 1)),
        (0, good.replace("callback_errors=0", "callback_errors=1")),
        (0, ""),
        (0, good.replace(" callback_errors=0", "")),
        (0, good.replace("drops=0", "drops=-1", 1)),
        (0, good + " drops=0"),
        (0, good.replace(PREFIX, "CoreAudio stats:")),
        (0, good.replace(" drops=0", "", 1)),
    ]
    for launcher_exit, stats_line in rejected:
        try:
            validate_result(launcher_exit, stats_line)
        except AudioResultError:
            continue
        raise AssertionError(
            f"invalid result accepted: exit={launcher_exit} line={stats_line!r}"
        )
    print(f"audio playback result self-test: PASS ({len(rejected) + 1} cases)")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--launcher-exit", type=int)
    parser.add_argument("--stats-line")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    if args.self_test:
        self_test()
        return 0
    if args.launcher_exit is None or args.stats_line is None:
        parser.error("--launcher-exit and --stats-line are required")

    try:
        values = validate_result(args.launcher_exit, args.stats_line)
    except AudioResultError as error:
        print(f"FAIL: {error}", file=sys.stderr)
        return 1
    print(*values)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
