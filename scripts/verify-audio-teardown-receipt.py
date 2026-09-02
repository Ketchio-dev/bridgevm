#!/usr/bin/env python3
"""Fail-closed verifier for the fixed N=10 B7 CoreAudio teardown receipt."""
from __future__ import annotations

import argparse
import copy
import json
import re
import sys
from datetime import datetime, timezone
from pathlib import Path

SCHEMA = "bridgevm.b7-coreaudio-teardown.v1"
PROFILE = "windows-hda-coreaudio-playback-shutdown-v1"
TIER = "t18-audio-teardown"
HASH_FIELDS = (
    "input_manifest_sha256", "image_sha256", "vars_sha256", "binary_sha256",
    "firmware_sha256", "playback_script_sha256", "run_log_set_sha256",
)
COUNT_FIELDS = (
    "sample_count", "required_run_count", "run_count", "passes", "failures",
    "frames_rendered_total", "drops_total", "queue_stop_errors_total",
    "queue_dispose_errors_total", "callback_errors_total",
    "callback_active_errors_total", "callback_stopping_errors_total",
    "callback_expected_stopping_errors_total", "callback_unexpected_errors_total",
    "callback_stopping_invalid_run_state_total", "callback_stopping_queue_invalidated_total",
    "callback_stopping_enqueue_during_reset_total", "callback_stopping_disposal_pending_total",
    "callback_stopping_unclassified_total", "elapsed_ms",
)
BOOLEAN_FIELDS = (
    "worker_cleanup_verified", "valid", "pass", "criterion_pass", "claim_eligible",
    "capability_promotion",
)
FIELDS = {
    "schema_version", "gate_id", "criterion", "tier", "job_id", "commit", "profile",
    *HASH_FIELDS, *COUNT_FIELDS, *BOOLEAN_FIELDS, "started_at", "finished_at",
    "host_model", "macos_version", "outcome", "failure_code",
}
SHA256 = re.compile(r"^[0-9a-f]{64}$")
COMMIT = re.compile(r"^[0-9a-f]{40}$")
IDENTIFIER = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$")
OUTCOMES = {"completed", "failed", "preflight-blocked", "canceled", "cleanup-failed", "missing-receipt"}
FAILURES = {"none", "invalid-input", "lane-failed", "cleanup-failed", "canceled", "missing-tier-receipt", "internal-error"}


class ReceiptError(ValueError):
    pass


def load_receipt(text: str) -> dict:
    def pairs(values: list[tuple[str, object]]) -> dict:
        result = {}
        for key, value in values:
            if key in result:
                raise ReceiptError(f"duplicate JSON key: {key}")
            result[key] = value
        return result
    value = json.loads(text, object_pairs_hook=pairs)
    if not isinstance(value, dict):
        raise ReceiptError("receipt must be an object")
    return value


def timestamp(raw: object, field: str) -> datetime:
    if not isinstance(raw, str) or re.fullmatch(r"[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z", raw) is None:
        raise ReceiptError(f"{field} is not exact UTC seconds")
    try:
        return datetime.strptime(raw, "%Y-%m-%dT%H:%M:%SZ").replace(tzinfo=timezone.utc)
    except ValueError as error:
        raise ReceiptError(f"{field} is invalid") from error


def validate(receipt: dict, expected_commit: str | None = None) -> None:
    if set(receipt) != FIELDS:
        raise ReceiptError("receipt has missing or unknown fields")
    fixed = {
        "schema_version": SCHEMA, "gate_id": "b7-coreaudio-teardown", "criterion": "B7",
        "tier": TIER, "profile": PROFILE,
    }
    for key, value in fixed.items():
        if receipt[key] != value:
            raise ReceiptError(f"{key} must be {value}")
    if not isinstance(receipt["job_id"], str) or IDENTIFIER.fullmatch(receipt["job_id"]) is None:
        raise ReceiptError("job_id is not canonical")
    if not isinstance(receipt["commit"], str) or COMMIT.fullmatch(receipt["commit"]) is None:
        raise ReceiptError("commit is not canonical")
    if expected_commit is not None and receipt["commit"] != expected_commit:
        raise ReceiptError("receipt commit differs from expected commit")
    for field in HASH_FIELDS:
        value = receipt[field]
        if value != "absent" and (not isinstance(value, str) or SHA256.fullmatch(value) is None):
            raise ReceiptError(f"{field} is not SHA-256 or absent")
    for field in COUNT_FIELDS:
        if isinstance(receipt[field], bool) or not isinstance(receipt[field], int) or receipt[field] < 0:
            raise ReceiptError(f"{field} is not a nonnegative integer")
    for field in BOOLEAN_FIELDS:
        if not isinstance(receipt[field], bool):
            raise ReceiptError(f"{field} is not boolean")
    for field in ("host_model", "macos_version"):
        value = receipt[field]
        if not isinstance(value, str) or not value or "/" in value or "\\" in value:
            raise ReceiptError(f"{field} is not safe public text")
    started, finished = timestamp(receipt["started_at"], "started_at"), timestamp(receipt["finished_at"], "finished_at")
    elapsed = int((finished - started).total_seconds() * 1000)
    if elapsed < 0 or abs(elapsed - receipt["elapsed_ms"]) > 1000:
        raise ReceiptError("timestamps and elapsed_ms disagree")
    if receipt["outcome"] not in OUTCOMES or receipt["failure_code"] not in FAILURES:
        raise ReceiptError("outcome or failure_code is unknown")
    if receipt["sample_count"] != 10 or receipt["required_run_count"] != 10:
        raise ReceiptError("B7 is fixed at ten runs")
    if receipt["run_count"] > 10 or receipt["passes"] + receipt["failures"] != receipt["run_count"]:
        raise ReceiptError("run totals do not reconcile")
    expected_typed = sum(receipt[field] for field in (
        "callback_stopping_invalid_run_state_total",
        "callback_stopping_enqueue_during_reset_total", "callback_stopping_disposal_pending_total",
    ))
    typed = expected_typed + receipt["callback_stopping_queue_invalidated_total"]
    if receipt["callback_errors_total"] != receipt["callback_active_errors_total"] + receipt["callback_stopping_errors_total"]:
        raise ReceiptError("callback total does not reconcile")
    if receipt["callback_expected_stopping_errors_total"] != expected_typed:
        raise ReceiptError("expected stopping total does not reconcile")
    if receipt["callback_stopping_errors_total"] != typed + receipt["callback_stopping_unclassified_total"]:
        raise ReceiptError("stopping total does not reconcile")
    if receipt["callback_unexpected_errors_total"] != (
        receipt["callback_active_errors_total"] + receipt["callback_stopping_queue_invalidated_total"]
        + receipt["callback_stopping_unclassified_total"]
    ):
        raise ReceiptError("unexpected total does not reconcile")
    computed = (
        receipt["valid"] and receipt["outcome"] == "completed" and receipt["failure_code"] == "none"
        and receipt["run_count"] == 10 and receipt["passes"] == 10 and receipt["failures"] == 0
        and receipt["frames_rendered_total"] > 0 and receipt["drops_total"] == 0
        and receipt["queue_stop_errors_total"] == 0 and receipt["queue_dispose_errors_total"] == 0
        and receipt["callback_active_errors_total"] == 0
        and receipt["callback_unexpected_errors_total"] == 0
        and receipt["callback_stopping_unclassified_total"] == 0
        and receipt["worker_cleanup_verified"]
        and all(receipt[field] != "absent" for field in HASH_FIELDS)
    )
    if receipt["pass"] != computed or receipt["criterion_pass"] != computed:
        raise ReceiptError("pass or criterion_pass disagrees with fixed B7 evidence")
    if receipt["claim_eligible"] or receipt["capability_promotion"]:
        raise ReceiptError("B7 receipt cannot promote product claims")
    if (receipt["failure_code"] == "none") != (receipt["outcome"] == "completed"):
        raise ReceiptError("failure_code must be none exactly for completed outcome")


def fixture(passing: bool = True) -> dict:
    value = {
        "schema_version": SCHEMA, "gate_id": "b7-coreaudio-teardown", "criterion": "B7", "tier": TIER,
        "job_id": "b7-fixture", "commit": "1" * 40, "profile": PROFILE,
        **{field: "2" * 64 for field in HASH_FIELDS},
        **{field: 0 for field in COUNT_FIELDS},
        "sample_count": 10, "required_run_count": 10, "run_count": 10, "passes": 10,
        "frames_rendered_total": 960_000, "callback_errors_total": 3,
        "callback_stopping_errors_total": 3, "callback_expected_stopping_errors_total": 3,
        "callback_stopping_enqueue_during_reset_total": 3,
        "worker_cleanup_verified": True, "valid": True, "pass": True, "criterion_pass": True,
        "claim_eligible": False, "capability_promotion": False,
        "started_at": "2026-09-01T12:00:00Z", "finished_at": "2026-09-01T12:10:00Z",
        "elapsed_ms": 600_000, "host_model": "Mac17,9", "macos_version": "26.5",
        "outcome": "completed", "failure_code": "none",
    }
    if not passing:
        value.update({
            "run_count": 1, "passes": 0, "failures": 1, "frames_rendered_total": 0,
            "callback_errors_total": 0, "callback_stopping_errors_total": 0,
            "callback_expected_stopping_errors_total": 0,
            "callback_stopping_enqueue_during_reset_total": 0,
            "worker_cleanup_verified": False, "valid": True, "pass": False,
            "criterion_pass": False, "outcome": "failed", "failure_code": "lane-failed",
        })
    return value


def self_test() -> int:
    checks = 0
    for value in (fixture(), fixture(False)):
        validate(value)
        checks += 1
    mutations = [
        ("sample_count", 9), ("passes", 9), ("queue_stop_errors_total", 1),
        ("queue_dispose_errors_total", 1), ("callback_active_errors_total", 1),
        ("callback_stopping_queue_invalidated_total", 1),
        ("callback_stopping_unclassified_total", 1), ("callback_errors_total", 4),
        ("criterion_pass", False), ("claim_eligible", True), ("image_sha256", "/private/disk.raw"),
    ]
    for field, value in mutations:
        changed = copy.deepcopy(fixture())
        changed[field] = value
        try:
            validate(changed)
        except ReceiptError:
            checks += 1
        else:
            raise AssertionError(f"accepted invalid {field}")
    changed = fixture(); changed["unknown"] = True
    try:
        validate(changed)
    except ReceiptError:
        checks += 1
    else:
        raise AssertionError("accepted unknown field")
    try:
        load_receipt('{"pass":true,"pass":false}')
    except ReceiptError:
        checks += 1
    else:
        raise AssertionError("accepted duplicate JSON key")
    print(f"PASS: B7 audio teardown receipt verifier ({checks} checks)")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("receipt", nargs="?", type=Path)
    parser.add_argument("--expected-commit")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        return self_test()
    if args.receipt is None:
        parser.error("receipt is required")
    try:
        validate(load_receipt(args.receipt.read_text(encoding="utf-8")), args.expected_commit)
    except (OSError, json.JSONDecodeError, ReceiptError) as error:
        print(f"FAIL: B7 audio teardown receipt: {error}", file=sys.stderr)
        return 1
    print("PASS: B7 audio teardown receipt")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
