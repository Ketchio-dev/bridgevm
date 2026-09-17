#!/usr/bin/env python3
"""Run one sealed native app CLI snapshot/restore marker pilot."""
from __future__ import annotations

from datetime import datetime, timezone
import hashlib
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys

from native_snapshot_restore_inputs import digest, prepare, reauthenticate
from native_snapshot_restore_receipt import initial, write_new


def file_digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def line_hash(path: Path) -> str:
    value = path.read_text(encoding="utf-8").strip()
    if len(value) != 64 or any(character not in "0123456789abcdef" for character in value):
        raise ValueError("invalid retained hash")
    return value


def shutdown_count(output: Path) -> int:
    count = 0
    for phase in ("phase1-original", "phase3-clobber", "phase5-restored"):
        log = output / phase / "run.log"
        if log.is_file() and "stop: PSCI " in log.read_text(encoding="utf-8", errors="replace"):
            count += 1
    return count


def main() -> int:
    if len(sys.argv) != 5:
        print("usage: run-native-snapshot-restore-tier.py OUT JOB_ID MANIFEST SEALED_BINARY", file=sys.stderr)
        return 2
    output, manifest, sealed_binary = map(Path, (sys.argv[1], sys.argv[3], sys.argv[4]))
    job_id = sys.argv[2]
    repo = Path(__file__).resolve().parents[2]
    commit = subprocess.check_output(["git", "-C", str(repo), "rev-parse", "HEAD"], text=True).strip()
    receipt = initial(job_id, commit)
    receipt["host_model"] = subprocess.check_output(["sysctl", "-n", "hw.model"], text=True).strip()
    receipt["macos_version"] = platform.mac_ver()[0]
    status = 1
    private = None
    try:
        prepared = output / "prepared-inputs"
        public, private = prepare(manifest, sealed_binary, commit, prepared)
        receipt.update(public)
        receipt["prepared_image_sha256"] = digest(prepared / "disk.raw")
        receipt["prepared_vars_sha256"] = digest(prepared / "vars.fd")
        environment = dict(
            os.environ,
            OUT=str(output),
            DISK=str(prepared / "disk.raw"),
            VARS=str(prepared / "vars.fd"),
            BRIDGEVM_PREBUILT_PROBE=private["binary"],
            NATIVE_SNAPSHOT_CLI=private["app_cli"],
            NATIVE_SNAPSHOT_VM_ID="a19-native-cli-live",
        )
        receipt["outcome"] = "failed"
        completed = subprocess.run(
            [str(repo / "scripts/verify-native-snapshot-restore-boots.sh")],
            cwd=repo, env=environment, check=False,
        )
        receipt["boots_attempted"] = sum((output / phase).is_dir() for phase in (
            "phase1-original", "phase3-clobber", "phase5-restored"))
        receipt["natural_shutdown_count"] = shutdown_count(output)
        receipt["boots_passed"] = receipt["natural_shutdown_count"]
        reauthenticate(private, sealed_binary)
        if completed.returncode != 0:
            raise RuntimeError(f"marker lifecycle exited {completed.returncode}")
        receipt.update({
            "final_disk_sha256": line_hash(output / "final-disk.sha256"),
            "final_vars_sha256": line_hash(output / "final-vars.sha256"),
            "snapshot_create_result_sha256": file_digest(output / "create.json"),
            "snapshot_restore_result_sha256": file_digest(output / "restore.json"),
            "original_marker_sha256": file_digest(output / "phase1-original/marker-after.txt"),
            "clobber_marker_sha256": file_digest(output / "phase3-clobber/marker-after.txt"),
            "restored_marker_sha256": file_digest(output / "phase5-restored/marker-before.txt"),
        })
        if receipt["original_marker_sha256"] != receipt["restored_marker_sha256"]:
            raise ValueError("restored marker hash differs from the original")
        if receipt["boots_passed"] != 3 or (output / "live").exists():
            raise ValueError("natural shutdown or live-library cleanup was not verified")
        shutil.rmtree(prepared)
        if prepared.exists() or prepared.is_symlink():
            raise ValueError("prepared media cleanup was not verified")
        receipt.update({"outcome": "completed", "pass": True, "run_count": 1,
                        "worker_cleanup_verified": True})
        status = 0
    except (OSError, UnicodeError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
        print("FAIL: native snapshot restore tier: " + str(error), file=sys.stderr)
    finally:
        if (output / "prepared-inputs").is_dir():
            shutil.rmtree(output / "prepared-inputs", ignore_errors=True)
        receipt["finished_at"] = datetime.now(timezone.utc).isoformat()
        write_new(output / "receipt.json", receipt)
    return status


if __name__ == "__main__":
    raise SystemExit(main())
