#!/usr/bin/env python3
"""Run one sealed native app CLI snapshot/restore marker pilot."""
from __future__ import annotations

from datetime import datetime, timezone
import os
from pathlib import Path
import platform
import subprocess
import sys

from a19_prepared_inputs_cleanup import PreparedInputsCleanup
from native_snapshot_restore_inputs import digest, prepare, reauthenticate
from native_snapshot_export_evidence import load_evidence, receipt_fields
from native_snapshot_restore_receipt import initial, write_new
from native_snapshot_restore_results import T20_PHASES, file_digest, line_hash, shutdown_count
from native_snapshot_restore_seal import merge_prepared, sealed_hashes


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
    prepared = output / "prepared-inputs"
    prepared_cleanup = PreparedInputsCleanup(prepared)
    children_stopped = True
    try:
        receipt.update(sealed_hashes(output, job_id, commit))
        public, private = prepare(manifest, sealed_binary, commit, prepared,
                                  on_created=prepared_cleanup.allocated)
        merge_prepared(receipt, public)
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
        children_stopped = False
        completed = subprocess.run(
            [str(repo / "scripts/verify-native-snapshot-restore-boots.sh")],
            cwd=repo, env=environment, check=False,
        )
        # A signal or interrupted wait cannot prove the shell's cleanup ran.
        children_stopped = completed.returncode >= 0
        receipt["boots_attempted"] = sum((output / phase).is_dir() for phase in T20_PHASES)
        receipt["natural_shutdown_count"] = shutdown_count(output, T20_PHASES)
        receipt["boots_passed"] = receipt["natural_shutdown_count"]
        reauthenticate(private, sealed_binary)
        if completed.returncode != 0:
            raise RuntimeError(f"marker lifecycle exited {completed.returncode}")
        export = load_evidence(output / "export-evidence.json", "a19-native-cli-live")
        receipt.update(receipt_fields(export))
        receipt.update({
            "final_disk_sha256": line_hash(output / "final-disk.sha256"), "final_vars_sha256": line_hash(output / "final-vars.sha256"),
            "snapshot_create_result_sha256": file_digest(output / "create.json"), "snapshot_restore_result_sha256": file_digest(output / "restore.json"),
            "original_marker_sha256": file_digest(output / "phase1-original/marker-after.txt"), "clobber_marker_sha256": file_digest(output / "phase3-clobber/marker-after.txt"),
            "restored_marker_sha256": file_digest(output / "phase5-restored/marker-before.txt"),
        })
        if (receipt["original_marker_sha256"] != receipt["restored_marker_sha256"] or
                receipt["clobber_marker_sha256"] == receipt["original_marker_sha256"]):
            raise ValueError("marker clobber and restore identity was not verified")
        if receipt["boots_passed"] != 3 or os.path.lexists(output / "live"):
            raise ValueError("natural shutdown or live-library cleanup was not verified")
        status = 0
    except (OSError, UnicodeError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
        print("FAIL: native snapshot restore tier: " + str(error), file=sys.stderr)
    finally:
        receipt["worker_cleanup_verified"] = prepared_cleanup.cleanup(output / "live", children_stopped)
        if status == 0 and receipt["worker_cleanup_verified"]:
            receipt.update({"outcome": "completed", "pass": True,
                            "run_count": 1, "sample_count": 1})
        else:
            status = 1
        receipt["finished_at"] = datetime.now(timezone.utc).isoformat()
        write_new(output / "receipt.json", receipt)
    return status


if __name__ == "__main__":
    raise SystemExit(main())
