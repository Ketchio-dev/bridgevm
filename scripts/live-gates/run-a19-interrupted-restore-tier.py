#!/usr/bin/env python3
"""One sealed, nonpromoting interrupted restore on private real-media clones."""
from __future__ import annotations

from datetime import datetime, timezone
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys

from a19_interrupted_restore_receipt import initial, validate, write_new
from a19_interrupt_cases import case_count
from a19_first_restore_collection import VM_ID, collect_first
from a19_auxiliary_collection import collect
from a19_lifecycle_process import LifecycleCleanupUncertain, run_lifecycle
from a19_interrupted_restore_seal import merge_prepared, sealed_hashes
from native_snapshot_restore_artifacts import RELATIONS, digest, regular
from native_snapshot_restore_inputs import prepare, reauthenticate
from native_snapshot_restore_results import shutdown_count
from a19_prepared_inputs_cleanup import PreparedInputsCleanup

PHASES = ("phase1-original", "phase3-clobber", "phase5-postkill", "phase7-restored")


def main() -> int:
    if len(sys.argv) != 5:
        print("usage: run-a19-interrupted-restore-tier.py OUT JOB_ID MANIFEST SEALED_BINARY", file=sys.stderr)
        return 2
    output, manifest, sealed_binary = map(Path, (sys.argv[1], sys.argv[3], sys.argv[4]))
    repo = Path(__file__).resolve().parents[2]
    commit = subprocess.check_output(["git", "-C", str(repo), "rev-parse", "HEAD"], text=True).strip()
    receipt = initial(sys.argv[2], commit)
    receipt["host_model"] = subprocess.check_output(["sysctl", "-n", "hw.model"], text=True).strip()
    receipt["macos_version"] = platform.mac_ver()[0]
    status = 1
    cleanup_proven = True
    prepared = output / "prepared-inputs"
    prepared_cleanup = PreparedInputsCleanup(prepared)
    try:
        receipt.update(sealed_hashes(output, sys.argv[2], commit))
        public, private = prepare(manifest, sealed_binary, commit, prepared,
                                  on_created=prepared_cleanup.allocated)
        merge_prepared(receipt, public)
        receipt["prepared_image_sha256"] = digest(prepared / "disk.raw")
        receipt["prepared_vars_sha256"] = digest(prepared / "vars.fd")
        pair = (prepared / "disk.raw").stat().st_size + (prepared / "vars.fd").stat().st_size
        if shutil.disk_usage(output).free <= pair + max(64 * 1024**2, pair // 10):
            raise ValueError("insufficient private APFS space")
        helper = Path(private["sealed_app"]) / RELATIONS["snapshot_helper"]
        regular(helper)
        environment = dict(
            os.environ, OUT=str(output), DISK=str(prepared / "disk.raw"),
            VARS=str(prepared / "vars.fd"), BRIDGEVM_PREBUILT_PROBE=private["binary"],
            NATIVE_SNAPSHOT_CLI=private["app_cli"], A19_SNAPSHOT_HELPER=str(helper),
            NATIVE_SNAPSHOT_VM_ID=VM_ID,
        )
        receipt["outcome"] = "failed"
        try:
            completed = run_lifecycle(repo, environment)
        except LifecycleCleanupUncertain:
            cleanup_proven = False
            raise
        receipt["boots_attempted"] = sum((output / phase).is_dir() for phase in PHASES)
        receipt["natural_shutdown_count"] = shutdown_count(output, PHASES)
        receipt["boots_passed"] = receipt["natural_shutdown_count"]
        reauthenticate(private, sealed_binary)
        if completed != 0:
            raise RuntimeError(f"T22 lifecycle exited {completed}")
        collect(output, receipt)
        receipt["final_prepared_image_sha256"] = digest(prepared / "disk.raw")
        receipt["final_prepared_vars_sha256"] = digest(prepared / "vars.fd")
        if receipt["boots_passed"] != 4 or (output / "live").exists():
            raise ValueError("four natural shutdowns or live library cleanup missing")
        status = 0
    except (OSError, UnicodeError, ValueError, RuntimeError, subprocess.SubprocessError,
            json.JSONDecodeError) as error:
        print(f"FAIL: T22 interrupted restore: {type(error).__name__}: {error}", file=sys.stderr)
    finally:
        receipt["worker_cleanup_verified"] = prepared_cleanup.cleanup(output / "live", cleanup_proven)
        if status == 0 and receipt["worker_cleanup_verified"]:
            candidate = {**receipt, "outcome": "completed", "pass": True,
                         "run_count": 1, "interruption_case_count": case_count(receipt), "sample_count": 1,
                         "finished_at": datetime.now(timezone.utc).isoformat()}
            try:
                validate(candidate, commit)
            except ValueError as error:
                status = 1
                print(f"FAIL: T22 passing receipt prerequisites: {error}", file=sys.stderr)
            else:
                receipt.update(candidate)
        else:
            status = 1
        receipt["finished_at"] = datetime.now(timezone.utc).isoformat()
        write_new(output / "receipt.json", receipt)
    return status


if __name__ == "__main__":
    raise SystemExit(main())
