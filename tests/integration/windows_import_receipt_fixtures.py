"""Synthetic public metadata; no app, guest media or live queue is involved."""
from __future__ import annotations

import importlib.util
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
COMMIT = "b" * 40
SPEC = importlib.util.spec_from_file_location("import_receipt_verifier", ROOT / "scripts/verify-windows-import-product-e2e-receipt.py")
VERIFIER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(VERIFIER)


def receipt(mode: str = "pilot") -> dict:
    count = 3 if mode == "release" else 1
    value = {
        "schema_version": VERIFIER.SCHEMA, "gate_id": VERIFIER.GATE_ID, "criterion": "A9", "tier": VERIFIER.TIER,
        "job_id": "import-self-test", "tested_commit": COMMIT, "commit": COMMIT,
        "hosted_ci_commit": COMMIT, "campaign_mode": mode, "artifact_signing_class": "development-signed",
        "clean_machine": False, "ui_frontend_automated": True, "product_model_automated": True,
        "three_d_injection": False, "worker_cleanup_verified": True, "hosted_ci_green": False,
        "security_ci_green": False, "valid": True, "expected_runs": count, "run_count": count, "passes": count,
        "failures": 0, "elapsed_ms": 0, "failure_code": "none", "outcome": "completed", "pass": True,
        "claim_eligible": False, "criterion_pass": False, "capability_promotion": False,
        "host_model": "fixture-host", "macos_version": "fixture-macos", "started_at": "2026-09-16T00:00:00Z",
        "finished_at": "2026-09-16T00:00:00Z", "hosted_ci_run_id": "absent", "security_ci_run_id": "absent",
    }
    value.update({field: "a" * 64 for field in VERIFIER.HASH_FIELDS})
    value.update({field: count for field in VERIFIER.STAGE_FIELDS})
    return value


def blocked_receipt() -> dict:
    value = receipt()
    value.update({"outcome": "preflight-blocked", "failure_code": "missing-vars", "pass": False,
                  "run_count": 0, "passes": 0, "ui_frontend_automated": False, "product_model_automated": False})
    value.update({field: "absent" for field in VERIFIER.HASH_FIELDS})
    value.update({field: 0 for field in VERIFIER.STAGE_FIELDS})
    return value
