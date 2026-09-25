"""Strict, path-free receipt for one real-media A19 byte-quota boundary case."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
from datetime import datetime, timezone
from pathlib import Path
from a19_quota_seal import read_bounded_regular, sealed_hashes

TIER = "t21-a19-quota-refusal"
HASHES = (
    "input_manifest_sha256", "app_artifact_sha256", "app_cli_sha256",
    "app_executable_sha256", "snapshot_helper_sha256", "image_sha256",
    "vars_sha256", "binary_hash", "prepared_image_sha256", "prepared_vars_sha256",
    "final_disk_sha256", "final_vars_sha256", "refusal_output_sha256",
    "snapshot_manifest_sha256", "snapshot_disk_sha256", "snapshot_vars_sha256",
)
REQUIRED = {
    "schema_version", "tier", "job_id", "commit", *HASHES,
    "app_profile", "binary_source_commit", "binary_profile", "binary_features",
    "rust_toolchain", "started_at", "finished_at", "host_model", "macos_version",
    "outcome", "pass", "pair_bytes", "rejected_quota_bytes", "accepted_quota_bytes",
    "refusal_exit_code", "refusal_reason", "refusal_destination_absent",
    "success_verified", "worker_cleanup_verified", "quota_case_count", "sample_count",
    "boots_attempted", "run_count", "claim_eligible", "criterion_pass",
    "capability_promotion", "three_d_injection",
}
SHA256 = re.compile(r"[0-9a-f]{64}\Z")
COMMIT = re.compile(r"[0-9a-f]{40}\Z")
JOB_ID = re.compile(r"[A-Za-z0-9][A-Za-z0-9._-]{0,127}\Z")


def initial(job_id: str, commit: str) -> dict:
    value = {key: "absent" for key in HASHES}
    value.update({
        "schema_version": 1, "tier": TIER, "job_id": job_id, "commit": commit,
        "app_profile": "release", "binary_source_commit": commit,
        "binary_profile": "release", "binary_features": "venus",
        "rust_toolchain": "1.97.0", "started_at": datetime.now(timezone.utc).isoformat(),
        "finished_at": "absent", "host_model": "absent", "macos_version": "absent",
        "outcome": "failed-before-receipt", "pass": False, "pair_bytes": 0,
        "rejected_quota_bytes": -1, "accepted_quota_bytes": -1,
        "refusal_exit_code": -1, "refusal_reason": "absent",
        "refusal_destination_absent": False, "success_verified": False,
        "worker_cleanup_verified": False, "quota_case_count": 0,
        "sample_count": 0, "boots_attempted": 0, "run_count": 0,
        "claim_eligible": False, "criterion_pass": False,
        "capability_promotion": False, "three_d_injection": False,
    })
    return value


def quota_error(pair_bytes: int) -> bytes:
    return (f"snapshot would write {pair_bytes} bytes, over the "
            f"{pair_bytes - 1} byte quota\n").encode()


def validate(value: object, expected_commit: str | None = None) -> dict:
    if not isinstance(value, dict) or set(value) != REQUIRED:
        raise ValueError("quota receipt has an unexpected field set")
    if type(value["schema_version"]) is not int or value["schema_version"] != 1 or value["tier"] != TIER:
        raise ValueError("quota receipt schema or tier is invalid")
    if not isinstance(value["job_id"], str) or not JOB_ID.fullmatch(value["job_id"]):
        raise ValueError("quota receipt job id is invalid")
    commit = value["commit"]
    if not isinstance(commit, str) or not COMMIT.fullmatch(commit) or value["binary_source_commit"] != commit:
        raise ValueError("quota receipt source commit is invalid")
    if expected_commit is not None and commit != expected_commit:
        raise ValueError("quota receipt differs from the sealed commit")
    for field, expected in (("app_profile", "release"), ("binary_profile", "release"),
                            ("binary_features", "venus"), ("rust_toolchain", "1.97.0")):
        if value[field] != expected:
            raise ValueError(f"quota receipt {field} is invalid")
    for field in HASHES:
        if value[field] != "absent" and (not isinstance(value[field], str) or not SHA256.fullmatch(value[field])):
            raise ValueError(f"quota receipt {field} is invalid")
    for field in ("pass", "refusal_destination_absent", "success_verified", "worker_cleanup_verified",
                  "claim_eligible", "criterion_pass", "capability_promotion", "three_d_injection"):
        if type(value[field]) is not bool:
            raise ValueError(f"quota receipt {field} must be boolean")
    if any(value[field] for field in ("claim_eligible", "criterion_pass", "capability_promotion", "three_d_injection")):
        raise ValueError("one quota case cannot promote A19 or enable 3D")
    for field in ("pair_bytes", "rejected_quota_bytes", "accepted_quota_bytes",
                  "refusal_exit_code", "quota_case_count", "sample_count", "boots_attempted", "run_count"):
        if type(value[field]) is not int:
            raise ValueError(f"quota receipt {field} must be integer")
    if (value["sample_count"], value["boots_attempted"]) != (0, 0) or value["run_count"] not in (0, 1):
        raise ValueError("quota receipt invents a guest lifecycle sample")
    if value["outcome"] not in ("failed-before-receipt", "failed", "completed"):
        raise ValueError("quota receipt outcome is invalid")
    if value["refusal_reason"] not in ("absent", "quota_exceeded"):
        raise ValueError("quota receipt refusal reason is invalid")
    if value["pass"]:
        pair = value["pair_bytes"]
        if not 1 <= pair <= (1 << 64) - 1:
            raise ValueError("passing quota receipt has invalid pair size")
        if (value["outcome"], value["run_count"], value["quota_case_count"]) != ("completed", 1, 1):
            raise ValueError("passing quota receipt has invalid run accounting")
        if (value["rejected_quota_bytes"], value["accepted_quota_bytes"], value["refusal_exit_code"],
            value["refusal_reason"]) != (pair - 1, pair, 1, "quota_exceeded"):
            raise ValueError("passing quota receipt lacks exact quota boundaries")
        if not all(value[field] for field in ("refusal_destination_absent", "success_verified", "worker_cleanup_verified")):
            raise ValueError("passing quota receipt lacks refusal, verification or cleanup")
        if any(value[field] == "absent" for field in HASHES):
            raise ValueError("passing quota receipt lacks authenticated hashes")
        if value["refusal_output_sha256"] != hashlib.sha256(quota_error(pair)).hexdigest():
            raise ValueError("passing quota receipt lacks the exact quota error")
        for field, source in (("prepared_image_sha256", "image_sha256"),
                              ("prepared_vars_sha256", "vars_sha256"),
                              ("final_disk_sha256", "image_sha256"),
                              ("final_vars_sha256", "vars_sha256"),
                              ("snapshot_disk_sha256", "image_sha256"),
                              ("snapshot_vars_sha256", "vars_sha256")):
            if value[field] != value[source]:
                raise ValueError(f"passing quota receipt changed {field}")
    elif value["quota_case_count"] != 0 or value["run_count"] != 0 or value["outcome"] == "completed":
        raise ValueError("failed quota receipt claims a completed case")
    return value


def load_receipt(path: Path) -> dict:
    return json.loads(read_bounded_regular(path, 65_536).decode("utf-8"))


def validate_seal(value: dict, job_dir: Path) -> None:
    if value["worker_cleanup_verified"] is not True:
        raise ValueError("quota receipt cannot publish before private-media cleanup")
    sealed = sealed_hashes(job_dir, value["job_id"], value["commit"])
    for field, expected in sealed.items():
        if value[field] != expected:
            raise ValueError(f"quota receipt differs from sealed {field}")


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
