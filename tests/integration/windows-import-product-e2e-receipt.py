#!/usr/bin/env python3
"""Deterministic semantic checks for the installed-disk import receipt."""
from __future__ import annotations
import json, pathlib, subprocess, sys, tempfile
from windows_import_receipt_fixtures import ROOT, VERIFIER, receipt

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

subprocess.run([sys.executable, ROOT / "tests/integration/windows-import-product-e2e-publication-contract.py"], check=True)
