#!/usr/bin/env python3
"""Build a public-safe B7 receipt by reauthenticating its manifest and private logs."""
from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import os
import platform
import re
import subprocess
import sys
import tempfile
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def module(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load {path.name}")
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


MANIFEST = module("bridgevm_b7_manifest", ROOT / "scripts/live-gates/audio-teardown-manifest.py")
LANE = module("bridgevm_b7_lane", ROOT / "scripts/audio-teardown-result.py")
VERIFY = module("bridgevm_b7_receipt", ROOT / "scripts/verify-audio-teardown-receipt.py")


def utc(raw: str) -> datetime:
    return datetime.strptime(raw, "%Y-%m-%dT%H:%M:%SZ").replace(tzinfo=timezone.utc)


def seal(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def host_value(command: list[str], fallback: str) -> str:
    try:
        value = subprocess.run(command, check=True, text=True, capture_output=True).stdout.strip()
        return value or fallback
    except (OSError, subprocess.CalledProcessError):
        return fallback


def collect(private: Path, attempts: int) -> tuple[list[dict], str]:
    if not 0 <= attempts <= 10:
        raise ValueError("attempts is outside the fixed campaign")
    if (private / f"lane-{attempts + 1}").exists():
        raise ValueError("private evidence exceeds declared attempts")
    passed: list[dict] = []
    records: list[str] = []
    for ordinal in range(1, attempts + 1):
        lane = private / f"lane-{ordinal}"
        run_log, exit_path, nonce_path = lane / "run.log", lane / "launcher.exit", lane / "nonce"
        try:
            raw_exit = exit_path.read_text(encoding="ascii")
            raw_nonce = nonce_path.read_text(encoding="ascii")
            if not re.fullmatch(r"[0-9]+\n", raw_exit) or not re.fullmatch(r"[0-9a-f]{64}\n", raw_nonce):
                raise ValueError("lane scalar evidence is malformed")
            nonce = raw_nonce.strip()
            result_file = lane / "share" / f"b7-audio-result-{nonce[:12]}.txt"
            if run_log.is_file() and result_file.is_file() and not run_log.is_symlink() and not result_file.is_symlink():
                records.append(f"{ordinal}\t{seal(run_log)}\t{seal(result_file)}\n")
            passed.append(LANE.validate(run_log, result_file, int(raw_exit), nonce, ordinal))
        except (OSError, UnicodeError, ValueError, LANE.AudioTeardownError):
            continue
    set_hash = hashlib.sha256("".join(records).encode()).hexdigest() if records else "absent"
    return passed, set_hash


def build(args: argparse.Namespace) -> dict:
    started = utc(args.started_at)
    finished = datetime.now(timezone.utc).replace(microsecond=0)
    if finished < started:
        raise ValueError("started-at is in the future")
    verified = MANIFEST.verify(args.manifest, ROOT, args.sealed_binary)
    if args.verified is not None:
        stored = json.loads(args.verified.read_text(encoding="utf-8"))
        if stored != verified:
            raise ValueError("stored manifest authentication is stale")
    manifest_valid = verified["valid"] is True
    lanes, log_set_hash = collect(args.private, args.attempts)
    sums = {field: sum(lane[field] for lane in lanes) for field in LANE.STAT_FIELDS}
    assets = verified.get("assets", {}) if manifest_valid else {}
    hashes = {
        "input_manifest_sha256": seal(args.manifest) if args.manifest.is_file() and not args.manifest.is_symlink() else "absent",
        "image_sha256": assets.get("image", {}).get("sha256", "absent"),
        "vars_sha256": assets.get("vars", {}).get("sha256", "absent"),
        "binary_sha256": assets.get("binary", {}).get("sha256", "absent"),
        "firmware_sha256": assets.get("firmware", {}).get("sha256", "absent"),
        "playback_script_sha256": assets.get("playback_script", {}).get("sha256", "absent"),
        "run_log_set_sha256": log_set_hash,
    }
    passes = len(lanes)
    receipt = {
        "schema_version": VERIFY.SCHEMA, "gate_id": "b7-coreaudio-teardown", "criterion": "B7",
        "tier": VERIFY.TIER, "job_id": args.job_id, "commit": args.commit, "profile": VERIFY.PROFILE,
        **hashes, "sample_count": 10, "required_run_count": 10, "run_count": args.attempts,
        "passes": passes, "failures": args.attempts - passes,
        "frames_rendered_total": sums.get("frames_rendered", 0),
        "drops_total": sums.get("drops", 0),
        "queue_stop_errors_total": sums.get("queue_stop_errors", 0),
        "queue_dispose_errors_total": sums.get("queue_dispose_errors", 0),
        "callback_errors_total": sums.get("callback_errors", 0),
        "callback_active_errors_total": sums.get("callback_active_errors", 0),
        "callback_stopping_errors_total": sums.get("callback_stopping_errors", 0),
        "callback_expected_stopping_errors_total": sums.get("callback_expected_stopping_errors", 0),
        "callback_unexpected_errors_total": sums.get("callback_unexpected_errors", 0),
        "callback_stopping_invalid_run_state_total": sums.get("callback_stopping_invalid_run_state", 0),
        "callback_stopping_queue_invalidated_total": sums.get("callback_stopping_queue_invalidated", 0),
        "callback_stopping_enqueue_during_reset_total": sums.get("callback_stopping_enqueue_during_reset", 0),
        "callback_stopping_disposal_pending_total": sums.get("callback_stopping_disposal_pending", 0),
        "callback_stopping_unclassified_total": sums.get("callback_stopping_unclassified", 0),
        "started_at": args.started_at, "finished_at": finished.strftime("%Y-%m-%dT%H:%M:%SZ"),
        "elapsed_ms": int((finished - started).total_seconds() * 1000),
        "host_model": host_value(["/usr/sbin/sysctl", "-n", "hw.model"], platform.machine() or "unknown"),
        "macos_version": host_value(["/usr/bin/sw_vers", "-productVersion"], platform.release() or "unknown"),
        "worker_cleanup_verified": args.cleanup_verified,
        "valid": manifest_valid, "outcome": args.outcome, "failure_code": args.failure_code,
        "pass": False, "criterion_pass": False, "claim_eligible": False, "capability_promotion": False,
    }
    computed = (
        manifest_valid and args.outcome == "completed" and args.failure_code == "none"
        and args.attempts == 10 and passes == 10 and args.cleanup_verified
    )
    receipt["pass"] = computed
    receipt["criterion_pass"] = computed
    VERIFY.validate(receipt, args.commit)
    return receipt


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--private", type=Path, required=True)
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--verified", type=Path)
    parser.add_argument("--sealed-binary", type=Path)
    parser.add_argument("--job-id", required=True)
    parser.add_argument("--commit", required=True)
    parser.add_argument("--started-at", required=True)
    parser.add_argument("--attempts", type=int, required=True)
    parser.add_argument("--outcome", choices=sorted(VERIFY.OUTCOMES), required=True)
    parser.add_argument("--failure-code", choices=sorted(VERIFY.FAILURES), required=True)
    parser.add_argument("--cleanup-verified", action="store_true")
    args = parser.parse_args()
    try:
        receipt = build(args)
        rendered = json.dumps(receipt, indent=2, sort_keys=True) + "\n"
        descriptor, temporary_name = tempfile.mkstemp(prefix=f".{args.out.name}.", dir=args.out.parent)
        temporary = Path(temporary_name)
        try:
            with os.fdopen(descriptor, "w", encoding="utf-8") as output:
                output.write(rendered); output.flush(); os.fsync(output.fileno())
            os.link(temporary, args.out)
        finally:
            temporary.unlink(missing_ok=True)
    except (OSError, UnicodeError, ValueError, json.JSONDecodeError, VERIFY.ReceiptError) as error:
        print(f"B7 receipt refused: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
