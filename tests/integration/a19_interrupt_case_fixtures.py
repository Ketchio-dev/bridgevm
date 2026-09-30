"""Synthetic schema 2 T22 receipts shared by the A19 interruption case contracts."""
from __future__ import annotations

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/live-gates"))
import a19_interrupt_cases as cases
import a19_interrupt_stop_points as points
import a19_interrupted_restore_receipt as receipt

COMMIT = "c" * 40
SHA_A, SHA_B, SHA_C, SHA_D, SHA_E, SHA_F = (letter * 64 for letter in "abcdef")
# Every top-level hash, the snapshot's included, is SHA_A. The swap case's old
# generation (SHA_E) differs from the snapshot its retry installs; the create
# destination was absent and its retry copied the SHA_B pair.
AGREEING = {
    "swap_": {"preinterrupt_disk_sha256": SHA_E, "preinterrupt_vars_sha256": SHA_E,
              "postkill_disk_sha256": SHA_E, "postkill_vars_sha256": SHA_E, "retry_result_sha256": SHA_F,
              "postretry_disk_sha256": SHA_A, "postretry_vars_sha256": SHA_A},
    "create_": {"source_disk_sha256": SHA_B, "source_vars_sha256": SHA_B,
                "preinterrupt_manifest_sha256": "absent", "postkill_manifest_sha256": "absent",
                "retry_result_sha256": SHA_F, "postretry_manifest_sha256": SHA_E,
                "postretry_disk_sha256": SHA_B, "postretry_vars_sha256": SHA_B},
}


def passing(**changes) -> dict:
    value = receipt.initial("cases-fixture", COMMIT)
    value.update(dict.fromkeys(receipt.HASHES, SHA_A), host_model="Mac17,9", macos_version="27.0",
                 finished_at=value["started_at"])
    value.update({field: True for field in receipt.FLAGS[:8]})
    value.update({"pass": True, "outcome": "completed", "interruption_stage": points.STAGED_RESTORE.name,
                  "boots_attempted": 4, "boots_passed": 4, "natural_shutdown_count": 4,
                  "interruption_case_count": 1, "sample_count": 1, "run_count": 1,
                  "clobber_marker_sha256": SHA_B, "postkill_marker_sha256": SHA_B,
                  "schema_version": 2, **cases.absent_cases()})
    value.update(changes)
    return value


def flagged(prefix: str, log: str) -> dict:
    """A stopped case with its flags and stop log but no content hashes."""
    return {prefix + "interruption_stage": cases.ADDED_POINTS[prefix].name,
            prefix + "stop_fd_log_sha256": log, **{prefix + flag: True for flag in cases.CASE_FLAGS}}


def proven(prefix: str, log: str) -> dict:
    return {**flagged(prefix, log), **{prefix + field: item for field, item in AGREEING[prefix].items()}}
