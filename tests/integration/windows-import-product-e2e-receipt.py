#!/usr/bin/env python3
"""Deterministic semantic checks for the installed-disk import receipt."""
from __future__ import annotations
import importlib.util, json, pathlib, subprocess, tempfile

ROOT = pathlib.Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("verifier", ROOT / "scripts/verify-windows-import-product-e2e-receipt.py")
VERIFIER = importlib.util.module_from_spec(SPEC); SPEC.loader.exec_module(VERIFIER)

def receipt() -> dict:
    value = {
        "schema_version": VERIFIER.SCHEMA, "gate_id": VERIFIER.GATE_ID, "criterion": "A9", "tier": VERIFIER.TIER,
        "job_id": "import-self-test", "tested_commit": "b" * 40, "commit": "b" * 40,
        "hosted_ci_commit": "b" * 40, "campaign_mode": "pilot", "artifact_signing_class": "development-signed",
        "clean_machine": False, "ui_frontend_automated": True, "product_model_automated": True,
        "three_d_injection": False, "worker_cleanup_verified": True, "hosted_ci_green": False,
        "security_ci_green": False, "valid": True, "expected_runs": 1, "run_count": 1, "passes": 1,
        "failures": 0, "elapsed_ms": 0, "failure_code": "none", "outcome": "completed", "pass": True,
        "claim_eligible": False, "criterion_pass": False, "capability_promotion": False,
        "host_model": "fixture-host", "macos_version": "fixture-macos", "started_at": "2026-09-16T00:00:00Z",
        "finished_at": "2026-09-16T00:00:00Z", "hosted_ci_run_id": "absent", "security_ci_run_id": "absent",
    }
    value.update({field: "a" * 64 for field in VERIFIER.HASH_FIELDS})
    value.update({field: 1 for field in VERIFIER.STAGE_FIELDS})
    return value

VERIFIER.validate(receipt(), expected_commit="b" * 40)
bad = receipt(); bad["claim_eligible"] = True
try: VERIFIER.validate(bad)
except VERIFIER.ReceiptError: pass
else: raise AssertionError("one journey granted combined A9 eligibility")
bad = receipt(); bad["source_disk_sha256"] = "absent"
try: VERIFIER.validate(bad)
except VERIFIER.ReceiptError: pass
else: raise AssertionError("missing authenticated source retained pass=true")
bad = receipt(); bad[VERIFIER.STAGE_FIELDS[5]] = 0; bad[VERIFIER.STAGE_FIELDS[6]] = 1
try: VERIFIER.validate(bad)
except VERIFIER.ReceiptError: pass
else: raise AssertionError("out-of-order stages were accepted")
print("PASS: installed-disk import receipt semantics")
with tempfile.TemporaryDirectory() as temporary:
    directory = pathlib.Path(temporary); (directory / "input-manifest.tsv").write_text("missing\n")
    (directory / "job.env").write_text("submitted_at=2026-09-16T00:00:00Z\n")
    subprocess.run([ROOT / "scripts/live-gates/write-windows-import-product-e2e-missing-receipt.sh",
        directory, ROOT, "missing-import", "b" * 40], check=True)
    missing = json.loads((directory / "receipt.json").read_text())
    VERIFIER.validate(missing, expected_commit="b" * 40)
    assert missing["outcome"] == "missing-receipt" and missing["pass"] is False
