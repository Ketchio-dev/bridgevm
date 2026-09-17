#!/usr/bin/env python3
"""Validate a public installed-disk import product E2E receipt."""
from __future__ import annotations
import argparse, json, re, sys
from datetime import datetime, timezone
from pathlib import Path

SCHEMA = "bridgevm.windows-hvf-import-product-e2e.v1"
GATE_ID = "windows-hvf-installed-disk-import-e2e"
TIER = "t19-windows-hvf-import-product-e2e"
HASH_FIELDS = (
    "input_manifest_sha256", "app_artifact_sha256", "app_executable_sha256", "helper_sha256",
    "runner_sha256", "source_disk_sha256", "source_vars_sha256", "source_vtpm_tree_sha256",
    "imported_initial_disk_sha256", "imported_initial_vars_sha256", "imported_initial_vtpm_tree_sha256",
    "final_disk_sha256", "final_vars_sha256", "final_vtpm_tree_sha256", "guest_evidence_sha256",
)
STAGE_FIELDS = tuple(f"{name}_passes" for name in (
    "artifact_preflight", "source_authenticated", "ui_imported", "imported_media_authenticated",
    "first_ready", "keyboard_pointer", "clipboard", "folder_share", "network", "audio",
    "first_shutdown", "snapshot_restore", "second_ready", "second_shutdown",
))
BOOLEAN_FIELDS = ("clean_machine", "ui_frontend_automated", "product_model_automated",
    "three_d_injection", "worker_cleanup_verified", "hosted_ci_green", "security_ci_green",
    "valid", "pass", "claim_eligible", "criterion_pass", "capability_promotion")
TEXT_FIELDS = ("job_id", "host_model", "macos_version", "started_at", "finished_at",
               "hosted_ci_run_id", "security_ci_run_id")
FAILURE_CODES = {"none", "missing-app-artifact", "missing-installed-disk", "missing-vars", "missing-vtpm",
    "invalid-vars", "hash-mismatch", "product-model-failed", "first-boot-failed", "integration-failed",
    "snapshot-failed", "cleanup-failed", "canceled", "missing-tier-receipt", "internal-error"}
OUTCOMES = {"completed", "preflight-blocked", "failed", "canceled", "cleanup-failed", "missing-receipt"}
SHA256 = re.compile(r"^[0-9a-f]{64}$"); COMMIT = re.compile(r"^[0-9a-f]{40}$")
IDENTIFIER = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$")
TIMESTAMP = re.compile(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$")
REQUIRED = frozenset({"schema_version", "gate_id", "criterion", "tier", "tested_commit", "commit",
    "hosted_ci_commit", "campaign_mode", "artifact_signing_class", "expected_runs", "run_count",
    "passes", "failures", "elapsed_ms", "failure_code", "outcome", *HASH_FIELDS, *STAGE_FIELDS,
    *BOOLEAN_FIELDS, *TEXT_FIELDS})

class ReceiptError(ValueError): pass

def unique(pairs):
    value = {}
    for key, item in pairs:
        if key in value: raise ReceiptError(f"duplicate field: {key}")
        value[key] = item
    return value

def integer(value, name, minimum=0):
    if isinstance(value, bool) or not isinstance(value, int) or value < minimum:
        raise ReceiptError(f"{name} must be an integer >= {minimum}")
    return value

def timestamp(value, name):
    if not isinstance(value, str) or not TIMESTAMP.fullmatch(value):
        raise ReceiptError(f"{name} is not a UTC timestamp")
    return datetime.strptime(value, "%Y-%m-%dT%H:%M:%SZ").replace(tzinfo=timezone.utc)

def validate(receipt, *, expected_commit=None, require_claim_eligible=False):
    if not isinstance(receipt, dict) or frozenset(receipt) != REQUIRED:
        raise ReceiptError("receipt has missing or unknown fields")
    fixed = {"schema_version": SCHEMA, "gate_id": GATE_ID, "criterion": "A9", "tier": TIER,
             "three_d_injection": False, "criterion_pass": False, "capability_promotion": False}
    if any(receipt.get(key) != value for key, value in fixed.items()): raise ReceiptError("fixed receipt identity differs")
    if not isinstance(receipt["job_id"], str) or not IDENTIFIER.fullmatch(receipt["job_id"]):
        raise ReceiptError("job_id is not canonical")
    for field in ("commit", "tested_commit", "hosted_ci_commit"):
        if not isinstance(receipt[field], str) or not COMMIT.fullmatch(receipt[field]):
            raise ReceiptError(f"{field} is not a full commit")
    if receipt["tested_commit"] != receipt["commit"] or expected_commit not in (None, receipt["commit"]):
        raise ReceiptError("tested commit identity differs")
    for field in HASH_FIELDS:
        if receipt[field] != "absent" and (not isinstance(receipt[field], str) or not SHA256.fullmatch(receipt[field])):
            raise ReceiptError(f"{field} is not a SHA-256")
    for field in BOOLEAN_FIELDS:
        if not isinstance(receipt[field], bool): raise ReceiptError(f"{field} is not boolean")
    for field in TEXT_FIELDS[1:]:
        if not isinstance(receipt[field], str) or not receipt[field] or "/" in receipt[field] or "\\" in receipt[field]:
            raise ReceiptError(f"{field} exposes a path or is empty")
    started, finished = timestamp(receipt["started_at"], "started_at"), timestamp(receipt["finished_at"], "finished_at")
    if finished < started or abs(integer(receipt["elapsed_ms"], "elapsed_ms") - int((finished-started).total_seconds()*1000)) > 1000:
        raise ReceiptError("elapsed time is inconsistent")
    mode = receipt["campaign_mode"]; expected = {"pilot": 1, "release": 3}.get(mode)
    if expected is None or integer(receipt["expected_runs"], "expected_runs", 1) != expected:
        raise ReceiptError("campaign size differs")
    count = integer(receipt["run_count"], "run_count"); passes = integer(receipt["passes"], "passes")
    failures = integer(receipt["failures"], "failures")
    if count > expected or passes + failures != count: raise ReceiptError("run totals are inconsistent")
    prior = count
    for field in STAGE_FIELDS:
        current = integer(receipt[field], field)
        if current > prior: raise ReceiptError(f"{field} exceeds the preceding stage")
        prior = current
    if receipt["failure_code"] not in FAILURE_CODES or receipt["outcome"] not in OUTCOMES:
        raise ReceiptError("failure or outcome is unknown")
    completed = receipt["outcome"] == "completed"
    if (receipt["failure_code"] == "none") != completed: raise ReceiptError("failure code contradicts outcome")
    eligible_run = (receipt["valid"] and completed and count == expected and passes == expected
        and all(receipt[field] == expected for field in STAGE_FIELDS) and receipt["worker_cleanup_verified"]
        and receipt["ui_frontend_automated"] and receipt["product_model_automated"]
        and all(receipt[field] != "absent" for field in HASH_FIELDS))
    if receipt["pass"] != eligible_run: raise ReceiptError("pass contradicts the retained evidence")
    if receipt["claim_eligible"]: raise ReceiptError("combined A9 eligibility is never granted by this receipt alone")
    if require_claim_eligible: raise ReceiptError("installed-disk receipt alone cannot close A9")
    return receipt

def main():
    parser = argparse.ArgumentParser(description=__doc__); parser.add_argument("receipt", type=Path)
    parser.add_argument("--expected-commit"); parser.add_argument("--require-claim-eligible", action="store_true")
    args = parser.parse_args()
    try:
        path = args.receipt
        if not path.is_file() or path.is_symlink() or not 0 < path.stat().st_size <= 1024 * 1024:
            raise ReceiptError("receipt is missing, unsafe, or oversized")
        receipt = json.loads(path.read_text(encoding="utf-8"), object_pairs_hook=unique)
        validate(receipt, expected_commit=args.expected_commit, require_claim_eligible=args.require_claim_eligible)
    except (OSError, UnicodeError, json.JSONDecodeError, ReceiptError) as error:
        print(f"invalid Windows import product receipt: {error}", file=sys.stderr); return 1
    print("PASS: Windows installed-disk import product receipt is valid"); return 0

if __name__ == "__main__": raise SystemExit(main())
