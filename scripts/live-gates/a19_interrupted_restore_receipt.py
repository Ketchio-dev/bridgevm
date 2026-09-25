#!/usr/bin/env python3
"""Path-free, nonpromoting receipt for one real-media interrupted restore."""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import re

from a19_interrupted_restore_seal import read_bounded_regular, sealed_hashes

TIER = "t22-a19-interrupted-restore"
HASHES = (
    "input_manifest_sha256", "app_artifact_sha256", "app_cli_sha256",
    "app_executable_sha256", "snapshot_helper_sha256", "image_sha256",
    "vars_sha256", "binary_hash", "prepared_image_sha256", "prepared_vars_sha256",
    "final_prepared_image_sha256", "final_prepared_vars_sha256",
    "snapshot_create_result_sha256", "snapshot_restore_retry_result_sha256",
    "snapshot_disk_sha256", "snapshot_vars_sha256",
    "preinterrupt_disk_sha256", "preinterrupt_vars_sha256",
    "postkill_disk_sha256", "postkill_vars_sha256",
    "postretry_disk_sha256", "postretry_vars_sha256",
    "postkill_export_result_sha256", "postkill_export_manifest_sha256",
    "postretry_export_result_sha256", "postretry_export_manifest_sha256",
    "original_marker_sha256", "clobber_marker_sha256",
    "postkill_marker_sha256", "restored_marker_sha256",
    "stop_fd_log_sha256",
)
FLAGS = (
    "pass", "helper_stop_verified", "staged_file_sync_order_verified",
    "old_selection_before_kill", "helper_killed_and_reaped",
    "postkill_pair_unchanged", "postretry_original_restored",
    "worker_cleanup_verified", "claim_eligible", "criterion_pass",
    "capability_promotion", "three_d_injection",
)
COUNTS = ("boots_attempted", "boots_passed", "natural_shutdown_count",
          "interruption_case_count", "sample_count", "run_count")
REQUIRED = {
    "schema_version", "tier", "job_id", "commit", *HASHES, *FLAGS, *COUNTS,
    "binary_source_commit", "binary_profile", "binary_features", "rust_toolchain",
    "app_profile", "started_at", "finished_at", "host_model", "macos_version",
    "outcome", "interruption_stage",
}
SHA256 = re.compile(r"[0-9a-f]{64}\Z")
COMMIT = re.compile(r"[0-9a-f]{40}\Z")
JOB_ID = re.compile(r"[A-Za-z0-9][A-Za-z0-9._-]{0,127}\Z")


def initial(job_id: str, commit: str) -> dict:
    value = {key: "absent" for key in HASHES}
    value.update({
        "schema_version": 1, "tier": TIER, "job_id": job_id, "commit": commit,
        "binary_source_commit": commit, "binary_profile": "release",
        "binary_features": "venus", "rust_toolchain": "1.97.0",
        "app_profile": "release", "started_at": datetime.now(timezone.utc).isoformat(),
        "finished_at": "absent", "host_model": "absent", "macos_version": "absent",
        "outcome": "failed-before-receipt", "interruption_stage": "absent",
        **{key: False for key in FLAGS}, **{key: 0 for key in COUNTS},
    })
    return value


def validate(value: object, expected_commit: str | None = None) -> dict:
    if not isinstance(value, dict) or set(value) != REQUIRED:
        raise ValueError("T22 receipt has an unexpected field set")
    if type(value["schema_version"]) is not int or value["schema_version"] != 1 or value["tier"] != TIER:
        raise ValueError("T22 receipt schema or tier is invalid")
    if not isinstance(value["job_id"], str) or not JOB_ID.fullmatch(value["job_id"]):
        raise ValueError("T22 receipt job id is invalid")
    commit = value["commit"]
    if not isinstance(commit, str) or not COMMIT.fullmatch(commit) or value["binary_source_commit"] != commit:
        raise ValueError("T22 receipt source commit is invalid")
    if expected_commit is not None and commit != expected_commit:
        raise ValueError("T22 receipt differs from sealed commit")
    for field, expected in (("app_profile", "release"), ("binary_profile", "release"),
                            ("binary_features", "venus"), ("rust_toolchain", "1.97.0")):
        if value[field] != expected:
            raise ValueError(f"T22 receipt {field} is invalid")
    for field in HASHES:
        if value[field] != "absent" and (not isinstance(value[field], str) or not SHA256.fullmatch(value[field])):
            raise ValueError(f"T22 receipt {field} is invalid")
    for field in FLAGS:
        if type(value[field]) is not bool:
            raise ValueError(f"T22 receipt {field} must be boolean")
    if any(value[field] for field in ("claim_eligible", "criterion_pass", "capability_promotion", "three_d_injection")):
        raise ValueError("one T22 observation cannot promote A19 or enable 3D")
    for field in COUNTS:
        if type(value[field]) is not int or value[field] < 0:
            raise ValueError(f"T22 receipt {field} is invalid")
    if value["outcome"] not in ("failed-before-receipt", "failed", "completed"):
        raise ValueError("T22 receipt outcome is invalid")
    if value["interruption_stage"] not in ("absent", "staged-disk-verify-read"):
        raise ValueError("T22 receipt stage is invalid")
    if not 0 <= value["boots_passed"] <= value["boots_attempted"] <= 4:
        raise ValueError("T22 receipt boot accounting is invalid")
    if not 0 <= value["natural_shutdown_count"] <= value["boots_passed"]:
        raise ValueError("T22 receipt shutdown accounting is invalid")
    if value["pass"]:
        if (value["outcome"], value["run_count"], value["interruption_case_count"],
            value["sample_count"]) != ("completed", 1, 1, 1):
            raise ValueError("passing T22 receipt lacks one complete case")
        if (value["boots_attempted"], value["boots_passed"], value["natural_shutdown_count"]) != (4, 4, 4):
            raise ValueError("passing T22 receipt lacks four natural shutdowns")
        if value["interruption_stage"] != "staged-disk-verify-read":
            raise ValueError("passing T22 receipt lacks authenticated stop stage")
        if any(value[field] == "absent" for field in HASHES) or not all(value[field] for field in FLAGS[:8]):
            raise ValueError("passing T22 receipt lacks hashes, stop proof or cleanup")
        for field, source in (("prepared_image_sha256", "image_sha256"),
                              ("prepared_vars_sha256", "vars_sha256"),
                              ("final_prepared_image_sha256", "image_sha256"),
                              ("final_prepared_vars_sha256", "vars_sha256"),
                              ("postkill_disk_sha256", "preinterrupt_disk_sha256"),
                              ("postkill_vars_sha256", "preinterrupt_vars_sha256"),
                              ("postkill_marker_sha256", "clobber_marker_sha256"),
                              ("restored_marker_sha256", "original_marker_sha256"),
                              ("postretry_disk_sha256", "snapshot_disk_sha256"),
                              ("postretry_vars_sha256", "snapshot_vars_sha256")):
            if value[field] != value[source]:
                raise ValueError(f"passing T22 receipt changed {field}")
        if value["original_marker_sha256"] == value["clobber_marker_sha256"]:
            raise ValueError("passing T22 receipt has indistinct markers")
    elif (value["outcome"] == "completed" or any(value[field] != 0 for field in
              ("interruption_case_count", "sample_count", "run_count"))):
        raise ValueError("failed T22 receipt claims a completed case")
    return value


def load_receipt(path: Path) -> dict:
    return json.loads(read_bounded_regular(path, 65_536).decode("utf-8"))


def validate_seal(value: dict, job_dir: Path) -> None:
    if value["worker_cleanup_verified"] is not True:
        raise ValueError("T22 receipt cannot publish before private-media cleanup")
    sealed = sealed_hashes(job_dir, value["job_id"], value["commit"])
    for field, expected in sealed.items():
        if value[field] != expected:
            raise ValueError(f"T22 receipt differs from sealed {field}")


def write_new(path: Path, value: dict) -> None:
    validate(value, value["commit"])
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    with os.fdopen(descriptor, "w", encoding="utf-8") as output:
        json.dump(value, output, indent=2, sort_keys=True)
        output.write("\n")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("verify", "missing"))
    parser.add_argument("path", type=Path)
    parser.add_argument("--expected-commit")
    parser.add_argument("--job-id")
    parser.add_argument("--job-dir", type=Path)
    args = parser.parse_args()
    if args.mode == "verify":
        value = validate(load_receipt(args.path), args.expected_commit)
        if args.job_dir is not None:
            validate_seal(value, args.job_dir)
        return 0
    if not args.job_id or not args.expected_commit:
        parser.error("missing mode requires --job-id and --expected-commit")
    value = initial(args.job_id, args.expected_commit)
    value["finished_at"] = datetime.now(timezone.utc).isoformat()
    write_new(args.path, value)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
