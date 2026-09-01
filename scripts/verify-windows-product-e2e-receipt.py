#!/usr/bin/env python3
"""Validate a public Windows-HVF 3D-off product E2E receipt."""
from __future__ import annotations

import argparse
import copy
import json
import re
import sys
from datetime import datetime, timezone
from pathlib import Path
SCHEMA = "bridgevm.windows-hvf-3d-off-product-e2e.v1"
GATE_ID = "windows-hvf-3d-off-product-e2e"
TIER = "t17-windows-hvf-product-e2e"
HASH_FIELDS = (
    "input_manifest_sha256",
    "app_artifact_sha256",
    "app_executable_sha256",
    "helper_sha256",
    "runner_sha256",
    "firmware_sha256",
    "secure_boot_policy_sha256",
    "iso_sha256",
    "installer_source_sha256",
    "bundled_vars_seed_sha256",
    "final_disk_sha256",
    "final_vars_sha256",
    "secure_boot_receipt_sha256",
    "guest_evidence_sha256",
)
STAGE_FIELDS = (
    "artifact_preflight_passes",
    "vm_created_passes",
    "source_prepared_passes",
    "windows_installed_passes",
    "secure_boot_provisioned_passes",
    "first_ready_passes",
    "keyboard_pointer_passes",
    "clipboard_passes",
    "folder_share_passes",
    "network_passes",
    "audio_passes",
    "first_shutdown_passes",
    "snapshot_restore_passes",
    "second_ready_passes",
    "second_shutdown_passes",
)
BOOLEAN_FIELDS = (
    "clean_machine",
    "ui_frontend_automated",
    "product_model_automated",
    "three_d_injection",
    "worker_cleanup_verified",
    "hosted_ci_green",
    "security_ci_green",
    "valid",
    "pass",
    "claim_eligible",
    "criterion_pass",
    "capability_promotion",
)
TEXT_FIELDS = (
    "job_id",
    "host_model",
    "macos_version",
    "started_at",
    "finished_at",
    "hosted_ci_run_id",
    "security_ci_run_id",
)
FAILURE_CODES = {
    "none",
    "missing-app-artifact",
    "missing-windows-iso",
    "missing-guest-payload",
    "missing-wimlib",
    "hash-mismatch",
    "product-model-failed",
    "installer-failed",
    "first-boot-failed",
    "integration-failed",
    "snapshot-failed",
    "cleanup-failed",
    "canceled",
    "missing-tier-receipt",
    "internal-error",
}
OUTCOMES = {"completed", "preflight-blocked", "failed", "canceled", "cleanup-failed", "missing-receipt"}
REQUIRED_FIELDS = frozenset(
    {
        "schema_version",
        "gate_id",
        "criterion",
        "tier",
        "tested_commit",
        "commit",
        "hosted_ci_commit",
        "campaign_mode",
        "artifact_signing_class",
        "expected_runs",
        "run_count",
        "passes",
        "failures",
        "elapsed_ms",
        "failure_code",
        "outcome",
        *HASH_FIELDS,
        *STAGE_FIELDS,
        *BOOLEAN_FIELDS,
        *TEXT_FIELDS,
    }
)
SHA256 = re.compile(r"^[0-9a-f]{64}$")
COMMIT = re.compile(r"^[0-9a-f]{40}$")
IDENTIFIER = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$")
TIMESTAMP = re.compile(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$")
class ReceiptError(ValueError):
    """The receipt is malformed or contradicts its own result."""


def _unique_object(pairs: list[tuple[str, object]]) -> dict:
    result: dict[str, object] = {}
    for key, value in pairs:
        if key in result:
            raise ReceiptError(f"duplicate field: {key}")
        result[key] = value
    return result


def load_receipt(text: str) -> object:
    return json.loads(text, object_pairs_hook=_unique_object)
def _integer(value: object, name: str, minimum: int = 0) -> int:
    if isinstance(value, bool) or not isinstance(value, int) or value < minimum:
        raise ReceiptError(f"{name} must be an integer >= {minimum}")
    return value


def _boolean(value: object, name: str) -> bool:
    if not isinstance(value, bool):
        raise ReceiptError(f"{name} must be boolean")
    return value


def _timestamp(value: object, name: str) -> datetime:
    if not isinstance(value, str) or not TIMESTAMP.fullmatch(value):
        raise ReceiptError(f"{name} must be a UTC second-resolution timestamp")
    return datetime.strptime(value, "%Y-%m-%dT%H:%M:%SZ").replace(tzinfo=timezone.utc)


def validate(receipt: object, *, expected_commit: str | None = None, require_claim_eligible: bool = False) -> dict:
    if not isinstance(receipt, dict):
        raise ReceiptError("receipt must be a JSON object")
    keys = frozenset(receipt)
    missing, unknown = sorted(REQUIRED_FIELDS - keys), sorted(keys - REQUIRED_FIELDS)
    if missing:
        raise ReceiptError(f"missing fields: {', '.join(missing)}")
    if unknown:
        raise ReceiptError(f"unknown fields: {', '.join(unknown)}")

    fixed = {"schema_version": SCHEMA, "gate_id": GATE_ID, "criterion": "A9", "tier": TIER}
    for field, value in fixed.items():
        if receipt[field] != value:
            raise ReceiptError(f"{field} must be {value!r}")
    if not isinstance(receipt["job_id"], str) or not IDENTIFIER.fullmatch(receipt["job_id"]):
        raise ReceiptError("job_id is not canonical")
    for field in ("commit", "tested_commit", "hosted_ci_commit"):
        if not isinstance(receipt[field], str) or not COMMIT.fullmatch(receipt[field]):
            raise ReceiptError(f"{field} must be a full lowercase commit")
    if receipt["tested_commit"] != receipt["commit"]:
        raise ReceiptError("tested_commit and commit differ")
    if expected_commit is not None and receipt["commit"] != expected_commit:
        raise ReceiptError("receipt commit does not match the expected commit")

    for field in HASH_FIELDS:
        value = receipt[field]
        if value != "absent" and (not isinstance(value, str) or not SHA256.fullmatch(value)):
            raise ReceiptError(f"{field} must be a lowercase SHA-256 or 'absent'")
    for field in TEXT_FIELDS[1:]:
        if not isinstance(receipt[field], str) or not receipt[field] or "/" in receipt[field] or "\\" in receipt[field]:
            raise ReceiptError(f"{field} must be non-empty public text, not a path")
    started = _timestamp(receipt["started_at"], "started_at")
    finished = _timestamp(receipt["finished_at"], "finished_at")
    if finished < started:
        raise ReceiptError("finished_at precedes started_at")

    mode = receipt["campaign_mode"]
    expected_for_mode = {"pilot": 1, "release": 3}
    if mode not in expected_for_mode:
        raise ReceiptError("campaign_mode must be pilot or release")
    expected_runs = _integer(receipt["expected_runs"], "expected_runs", 1)
    if expected_runs != expected_for_mode[mode]:
        raise ReceiptError(f"{mode} mode requires expected_runs={expected_for_mode[mode]}")
    run_count = _integer(receipt["run_count"], "run_count")
    passes = _integer(receipt["passes"], "passes")
    failures = _integer(receipt["failures"], "failures")
    elapsed_ms = _integer(receipt["elapsed_ms"], "elapsed_ms")
    if run_count > expected_runs or passes + failures != run_count:
        raise ReceiptError("run totals are inconsistent")
    wall_ms = int((finished - started).total_seconds() * 1000)
    if abs(wall_ms - elapsed_ms) > 1000:
        raise ReceiptError("elapsed_ms disagrees with the UTC timestamps")

    previous = run_count
    for field in STAGE_FIELDS:
        count = _integer(receipt[field], field)
        if count > previous:
            raise ReceiptError(f"{field} exceeds its preceding stage")
        previous = count
    for field in BOOLEAN_FIELDS:
        _boolean(receipt[field], field)

    signing = receipt["artifact_signing_class"]
    if signing not in ("development-ad-hoc", "developer-id-notarized"):
        raise ReceiptError("artifact_signing_class is not recognized")
    if receipt["outcome"] not in OUTCOMES or receipt["failure_code"] not in FAILURE_CODES:
        raise ReceiptError("outcome or failure_code is not recognized")
    completed = receipt["outcome"] == "completed"
    if (receipt["failure_code"] == "none") != completed:
        raise ReceiptError("failure_code must be none exactly for completed outcomes")
    if receipt["three_d_injection"]:
        raise ReceiptError("the General Preview E2E must keep 3D injection disabled")
    if receipt["criterion_pass"] or receipt["capability_promotion"]:
        raise ReceiptError("a 3D-off E2E cannot close A9 or promote product state")

    computed_pass = (
        receipt["valid"]
        and completed
        and run_count == expected_runs
        and passes == expected_runs
        and failures == 0
        and previous == expected_runs
        and receipt["product_model_automated"]
        and receipt["ui_frontend_automated"]
        and receipt["worker_cleanup_verified"]
        and all(receipt[field] != "absent" for field in HASH_FIELDS)
    )
    if receipt["pass"] != computed_pass:
        raise ReceiptError("pass does not match the complete stage evidence")
    computed_claim = (
        mode == "release"
        and computed_pass
        and receipt["clean_machine"]
        and receipt["ui_frontend_automated"]
        and signing == "developer-id-notarized"
        and receipt["hosted_ci_green"]
        and receipt["security_ci_green"]
        and receipt["hosted_ci_commit"] == receipt["commit"]
        and receipt["hosted_ci_run_id"] != "absent"
        and receipt["security_ci_run_id"] != "absent"
    )
    if receipt["claim_eligible"] != computed_claim:
        raise ReceiptError("claim_eligible does not match the fixed release rule")
    if require_claim_eligible and not receipt["claim_eligible"]:
        raise ReceiptError("receipt is valid but not claim-eligible")
    return receipt


def _fixture(mode: str = "release") -> dict:
    expected = 3 if mode == "release" else 1
    receipt = {
        "schema_version": SCHEMA,
        "gate_id": GATE_ID,
        "criterion": "A9",
        "tier": TIER,
        "job_id": "t17-self-test",
        "tested_commit": "1" * 40,
        "commit": "1" * 40,
        "hosted_ci_commit": "1" * 40,
        "campaign_mode": mode,
        "artifact_signing_class": "developer-id-notarized",
        "clean_machine": True,
        "ui_frontend_automated": True,
        "product_model_automated": True,
        "three_d_injection": False,
        "worker_cleanup_verified": True,
        "hosted_ci_green": True,
        "security_ci_green": True,
        "valid": True,
        "expected_runs": expected,
        "run_count": expected,
        "passes": expected,
        "failures": 0,
        "elapsed_ms": 120_000,
        "failure_code": "none",
        "outcome": "completed",
        "pass": True,
        "claim_eligible": mode == "release",
        "criterion_pass": False,
        "capability_promotion": False,
        "host_model": "Mac16,9",
        "macos_version": "26.5.2",
        "started_at": "2026-09-01T00:00:00Z",
        "finished_at": "2026-09-01T00:02:00Z",
        "hosted_ci_run_id": "ci-123",
        "security_ci_run_id": "security-456",
    }
    receipt.update({field: "ab" * 32 for field in HASH_FIELDS})
    receipt.update({field: expected for field in STAGE_FIELDS})
    return receipt


def self_test() -> int:
    checks = 0

    def accepts(receipt: dict, *, claim: bool = False) -> None:
        nonlocal checks
        validate(receipt, require_claim_eligible=claim)
        checks += 1

    def rejects(receipt: object, *, claim: bool = False) -> None:
        nonlocal checks
        try:
            validate(receipt, require_claim_eligible=claim)
        except ReceiptError:
            checks += 1
            return
        raise AssertionError("accepted invalid receipt fixture")

    release = _fixture()
    accepts(release, claim=True)
    pilot = _fixture("pilot")
    pilot.update({"artifact_signing_class": "development-ad-hoc", "clean_machine": False, "claim_eligible": False})
    accepts(pilot)
    rejects(pilot, claim=True)
    blocked = _fixture("pilot")
    blocked.update({"run_count": 0, "passes": 0, "elapsed_ms": 0, "outcome": "preflight-blocked", "failure_code": "missing-windows-iso", "pass": False, "claim_eligible": False, "product_model_automated": False, "worker_cleanup_verified": False, "finished_at": blocked["started_at"]})
    blocked.update({field: "absent" for field in HASH_FIELDS})
    blocked.update({field: 0 for field in STAGE_FIELDS})
    accepts(blocked)
    mutations: list[tuple[str, object]] = [
        ("three_d_injection", True),
        ("expected_runs", 2),
        ("run_count", 2),
        ("passes", 2),
        ("failures", 1),
        ("clipboard_passes", 2),
        ("iso_sha256", "/Users/example/Windows.iso"),
        ("tested_commit", "2" * 40),
        ("hosted_ci_commit", "2" * 40),
        ("claim_eligible", False),
        ("criterion_pass", True),
        ("capability_promotion", True),
        ("worker_cleanup_verified", False), ("ui_frontend_automated", False),
        ("elapsed_ms", 1),
        ("finished_at", "2026-08-31T23:59:59Z"),
    ]
    for field, value in mutations:
        changed = copy.deepcopy(release)
        changed[field] = value
        rejects(changed)
    changed = copy.deepcopy(release)
    changed["future_field"] = "not reviewed"
    rejects(changed)
    rejects([])
    try:
        load_receipt('{"pass":true,"pass":false}')
    except ReceiptError:
        checks += 1
    else:
        raise AssertionError("accepted duplicate JSON field")
    accepts(load_receipt(json.dumps(release)))
    print(f"PASS: Windows product E2E receipt verifier ({checks} checks)")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("receipt", nargs="?", type=Path)
    parser.add_argument("--expected-commit")
    parser.add_argument("--require-claim-eligible", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        return self_test()
    if args.receipt is None:
        parser.error("receipt is required unless --self-test is used")
    try:
        validate(load_receipt(args.receipt.read_text(encoding="utf-8")),
                 expected_commit=args.expected_commit,
                 require_claim_eligible=args.require_claim_eligible)
    except (OSError, json.JSONDecodeError, ReceiptError) as error:
        print(f"FAIL: Windows product E2E receipt: {error}", file=sys.stderr)
        return 1
    print("PASS: Windows product E2E receipt")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
