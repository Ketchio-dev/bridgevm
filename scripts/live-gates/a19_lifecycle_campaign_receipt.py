"""Fixed ten-lane A19 product lifecycle campaign receipt; lanes are never replaced.

The verdict mirrors B7 (T18): a fixed N=10, sequential independent lanes, stop
at the first failed lane, and pass only for 10/10. Per-lane evidence is flat
lists of hashes, counts and flags, one entry per attempted lane in order.
claim_eligible and criterion_pass stay false: this receipt cannot see the
release head, hosted CI, artifact signing or A19's quota and atomicity parts.
"""
from __future__ import annotations

from datetime import datetime, timezone
import json
import os
from pathlib import Path
import re

from a19_lifecycle_campaign_record import COUNTS, LANES, MARKERS, PAIR, SHA256, check_lane
from native_snapshot_restore_public import validate_public_fields

TIER = "t23-a19-lifecycle-campaign"
SEALED = ("input_manifest_sha256", "app_artifact_sha256", "app_cli_sha256", "app_executable_sha256",
          "snapshot_helper_sha256", "image_sha256", "vars_sha256", "binary_hash")
LIST_SOURCES = {
    "lane_ordinals": "ordinal", "lane_pass": "pass", "lane_cleanup_verified": "cleanup_verified",
    "lane_boots_attempted": "boots_attempted", "lane_boots_passed": "boots_passed",
    "lane_natural_shutdown_counts": "natural_shutdown_count",
    **{f"lane_{field}": field for field in (*PAIR, *MARKERS)},
}
LANE_FIELDS = (*LIST_SOURCES, "lane_record_sha256")
FLAGS = ("pass", "worker_cleanup_verified", "claim_eligible", "criterion_pass",
         "capability_promotion", "three_d_injection")
NEVER = FLAGS[2:]
TOTALS = ("run_count", "passes", "failures", *COUNTS)
FIXED = {
    "schema_version": "bridgevm.a19-lifecycle-campaign.v1", "tier": TIER, "criterion": "A19",
    "gate_id": "a19-product-lifecycle-campaign", "replacement_policy": "none",
    "sample_count": LANES, "required_run_count": LANES, "app_profile": "release",
    "binary_profile": "release", "binary_features": "venus", "rust_toolchain": "1.97.0",
}
OUTCOMES = {
    "completed": ("none",), "failed": ("lane-failed", "cleanup-failed", "internal-error"),
    "preflight-blocked": ("invalid-input",), "canceled": ("canceled",),
    "failed-before-receipt": ("missing-tier-receipt",),
}
REQUIRED = {*FIXED, "job_id", "commit", "binary_source_commit", *SEALED, *FLAGS, *TOTALS,
            *LANE_FIELDS, "started_at", "finished_at", "host_model", "macos_version",
            "outcome", "failure_code"}
COMMIT = re.compile(r"[0-9a-f]{40}\Z")
JOB_ID = re.compile(r"[A-Za-z0-9][A-Za-z0-9._-]{0,127}\Z")


def initial(job_id: str, commit: str) -> dict:
    return {**FIXED, "job_id": job_id, "commit": commit, "binary_source_commit": commit,
            **dict.fromkeys(SEALED, "absent"), **dict.fromkeys(FLAGS, False),
            **dict.fromkeys(TOTALS, 0), **{field: [] for field in LANE_FIELDS},
            "started_at": datetime.now(timezone.utc).isoformat(), "finished_at": "absent",
            "host_model": "absent", "macos_version": "absent",
            "outcome": "failed-before-receipt", "failure_code": "missing-tier-receipt"}


def lane_lists(collected: list[tuple[dict, str]]) -> dict[str, list]:
    lists = {name: [lane[source] for lane, _ in collected] for name, source in LIST_SOURCES.items()}
    lists["lane_record_sha256"] = [digest for _, digest in collected]
    return lists


def aggregate(value: dict, collected: list[tuple[dict, str]]) -> None:
    """Receipt lane lists and totals from retained records, in lane order."""
    value.update(lane_lists(collected))
    lanes = [lane for lane, _ in collected]
    value["run_count"] = len(lanes)
    value["passes"] = sum(lane["pass"] for lane in lanes)
    value["failures"] = value["run_count"] - value["passes"]
    for field in COUNTS:
        value[field] = sum(lane[field] for lane in lanes)


def computed_pass(value: dict) -> bool:
    markers = value["lane_original_marker_sha256"] + value["lane_clobber_marker_sha256"]
    return bool(value["outcome"] == "completed" and value["run_count"] == value["passes"] == LANES
                and value["worker_cleanup_verified"] is True and all(value["lane_pass"])
                and all(value[field] != "absent" for field in SEALED)
                and len(set(markers)) == 2 * LANES)


def validate_lanes(value: dict) -> None:
    count = value["run_count"]
    if count > LANES:
        raise ValueError("T23 receipt exceeds the fixed ten lanes")
    for field in LANE_FIELDS:
        if type(value[field]) is not list or len(value[field]) != count:
            raise ValueError(f"T23 receipt {field} lacks exactly one entry per attempted lane")
    ordinals = value["lane_ordinals"]
    if any(type(item) is not int for item in ordinals) or ordinals != list(range(1, count + 1)):
        raise ValueError("T23 lane ordinals are missing, duplicated or out of order")
    for index in range(count):
        lane = {source: value[name][index] for name, source in LIST_SOURCES.items()}
        check_lane(lane, (*PAIR, *MARKERS))
        digest = value["lane_record_sha256"][index]
        if not isinstance(digest, str) or not SHA256.fullmatch(digest):
            raise ValueError("T23 lane record hash is invalid")
        if lane["pass"] and (lane["prepared_image_sha256"], lane["prepared_vars_sha256"]) != (
                value["image_sha256"], value["vars_sha256"]):
            raise ValueError("a passing T23 lane did not clone the sealed disk and vars")
        if not lane["pass"] and index != count - 1:
            raise ValueError("a failed T23 lane was followed by another lane; lanes are never replaced")
    if value["passes"] != sum(value["lane_pass"]) or value["failures"] != count - value["passes"]:
        raise ValueError("T23 pass and failure totals do not reconcile with the lanes")
    lists = {source: name for name, source in LIST_SOURCES.items()}
    if any(value[field] != sum(value[lists[field]]) for field in COUNTS):
        raise ValueError("T23 boot and natural shutdown totals do not reconcile with the lanes")


def validate(value: object, expected_commit: str | None = None) -> dict:
    if not isinstance(value, dict) or set(value) != REQUIRED:
        raise ValueError("T23 receipt has an unexpected field set")
    for field, expected in FIXED.items():
        if type(value[field]) is not type(expected) or value[field] != expected:
            raise ValueError(f"T23 receipt {field} is invalid")
    if not isinstance(value["job_id"], str) or not JOB_ID.fullmatch(value["job_id"]):
        raise ValueError("T23 receipt job id is invalid")
    commit = value["commit"]
    if not isinstance(commit, str) or not COMMIT.fullmatch(commit) or value["binary_source_commit"] != commit:
        raise ValueError("T23 receipt source commit is invalid")
    if expected_commit is not None and commit != expected_commit:
        raise ValueError("T23 receipt differs from the sealed commit")
    for field in SEALED:
        if value[field] != "absent" and (not isinstance(value[field], str) or not SHA256.fullmatch(value[field])):
            raise ValueError(f"T23 receipt {field} is invalid")
    for field in FLAGS:
        if type(value[field]) is not bool:
            raise ValueError(f"T23 receipt {field} must be boolean")
    if any(value[field] for field in NEVER):
        raise ValueError("a T23 receipt cannot claim eligibility, close A19, promote product state or enable 3D")
    validate_public_fields(value)
    for field in TOTALS:
        if type(value[field]) is not int or value[field] < 0:
            raise ValueError(f"T23 receipt {field} is invalid")
    if value["outcome"] not in OUTCOMES or value["failure_code"] not in OUTCOMES[value["outcome"]]:
        raise ValueError("T23 receipt outcome and failure code disagree")
    validate_lanes(value)
    if value["outcome"] == "completed" and (value["run_count"], value["failures"]) != (LANES, 0):
        raise ValueError("a completed T23 campaign needs ten lanes and no failure")
    if value["failure_code"] == "lane-failed" and (value["run_count"] == 0 or value["lane_pass"][-1]):
        raise ValueError("a lane-failed T23 receipt must end at its failed lane")
    if value["failure_code"] == "cleanup-failed" and value["worker_cleanup_verified"] and (
            value["run_count"] == 0 or value["lane_cleanup_verified"][-1]):
        raise ValueError("a cleanup-failed T23 receipt must show the unverified cleanup")
    if value["pass"] != computed_pass(value):
        raise ValueError("T23 pass disagrees with the fixed ten-lane evidence")
    return value


def write_new(path: Path, value: dict) -> None:
    validate(value, value["commit"])
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    with os.fdopen(descriptor, "w", encoding="utf-8") as output:
        json.dump(value, output, indent=2, sort_keys=True)
        output.write("\n")
