"""Schema and fallback writer for the native app snapshot restore live tier."""
from __future__ import annotations

import argparse
import json
import re
from datetime import datetime, timezone
from pathlib import Path

TIER = "t20-a19-native-snapshot-restore"
HASHES = (
    "input_manifest_sha256", "app_artifact_sha256", "app_cli_sha256",
    "app_executable_sha256", "snapshot_helper_sha256", "image_sha256",
    "vars_sha256", "binary_hash", "prepared_image_sha256", "prepared_vars_sha256",
    "final_disk_sha256", "final_vars_sha256", "snapshot_create_result_sha256",
    "snapshot_restore_result_sha256", "snapshot_export_result_sha256", "snapshot_export_manifest_sha256",
    "exported_disk_sha256", "exported_vars_sha256", "original_marker_sha256", "clobber_marker_sha256", "restored_marker_sha256",
)
REQUIRED = {
    "schema_version", "tier", "job_id", "commit", *HASHES,
    "binary_source_commit", "binary_profile", "binary_features", "rust_toolchain",
    "started_at", "finished_at", "host_model", "macos_version", "outcome", "pass",
    "boots_attempted", "boots_passed", "natural_shutdown_count", "sample_count",
    "run_count", "claim_eligible", "criterion_pass", "capability_promotion",
    "three_d_injection", "worker_cleanup_verified",
}
SHA256 = re.compile(r"^[0-9a-f]{64}$")
COMMIT = re.compile(r"^[0-9a-f]{40}$")


def initial(job_id: str, commit: str) -> dict:
    value = {key: "absent" for key in HASHES}
    value.update({
        "schema_version": 1, "tier": TIER, "job_id": job_id, "commit": commit,
        "binary_source_commit": commit, "binary_profile": "release",
        "binary_features": "venus", "rust_toolchain": "1.97.0",
        "started_at": datetime.now(timezone.utc).isoformat(), "finished_at": "absent",
        "host_model": "absent", "macos_version": "absent", "outcome": "failed-before-receipt",
        "pass": False, "boots_attempted": 0, "boots_passed": 0,
        "natural_shutdown_count": 0, "sample_count": 1, "run_count": 0,
        "claim_eligible": False, "criterion_pass": False, "capability_promotion": False,
        "three_d_injection": False, "worker_cleanup_verified": False,
    })
    return value


def validate(value: object, expected_commit: str | None = None) -> dict:
    if not isinstance(value, dict) or set(value) != REQUIRED:
        raise ValueError("receipt has an unexpected field set")
    if value["schema_version"] != 1 or value["tier"] != TIER:
        raise ValueError("receipt schema or tier is invalid")
    if not isinstance(value["job_id"], str) or not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]{0,127}", value["job_id"]):
        raise ValueError("receipt job id is invalid")
    if not isinstance(value["commit"], str) or not COMMIT.fullmatch(value["commit"]):
        raise ValueError("receipt commit is invalid")
    if expected_commit is not None and value["commit"] != expected_commit:
        raise ValueError("receipt commit does not match the sealed worktree")
    if value["binary_source_commit"] != value["commit"]:
        raise ValueError("binary source identity differs from the tier commit")
    for field in HASHES:
        if value[field] != "absent" and (not isinstance(value[field], str) or not SHA256.fullmatch(value[field])):
            raise ValueError(f"receipt {field} is invalid")
    for field in ("pass", "claim_eligible", "criterion_pass", "capability_promotion", "three_d_injection", "worker_cleanup_verified"):
        if not isinstance(value[field], bool):
            raise ValueError(f"receipt {field} must be boolean")
    if any(value[field] is not False for field in ("claim_eligible", "criterion_pass", "capability_promotion", "three_d_injection")):
        raise ValueError("a pilot receipt cannot promote a criterion or enable 3D")
    for field in ("boots_attempted", "boots_passed", "natural_shutdown_count", "sample_count", "run_count"):
        if not isinstance(value[field], int) or value[field] < 0:
            raise ValueError(f"receipt {field} is invalid")
    if value["sample_count"] != 1 or value["run_count"] not in (0, 1):
        raise ValueError("receipt sample accounting is invalid")
    if value["pass"]:
        if value["outcome"] != "completed" or value["run_count"] != 1:
            raise ValueError("passing receipt has invalid outcome accounting")
        if (value["boots_attempted"], value["boots_passed"], value["natural_shutdown_count"]) != (3, 3, 3):
            raise ValueError("passing receipt lacks all three natural shutdowns")
        if not value["worker_cleanup_verified"] or any(value[field] == "absent" for field in HASHES):
            raise ValueError("passing receipt lacks cleanup or an authenticated artifact")
    return value


def write_new(path: Path, value: dict) -> None:
    validate(value, value["commit"])
    with path.open("x", encoding="utf-8") as output:
        json.dump(value, output, indent=2, sort_keys=True)
        output.write("\n")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("verify", "missing"))
    parser.add_argument("path", type=Path)
    parser.add_argument("--expected-commit")
    parser.add_argument("--job-id")
    args = parser.parse_args()
    if args.mode == "verify":
        validate(json.loads(args.path.read_text(encoding="utf-8")), args.expected_commit)
        return 0
    if not args.job_id or not args.expected_commit:
        parser.error("missing mode requires --job-id and --expected-commit")
    value = initial(args.job_id, args.expected_commit)
    value["finished_at"] = datetime.now(timezone.utc).isoformat()
    write_new(args.path, value)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
