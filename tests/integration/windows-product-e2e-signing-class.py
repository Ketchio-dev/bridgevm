#!/usr/bin/env python3
"""Signing classification and non-promoting receipt contracts; no real certificates."""
import json
import os
from pathlib import Path
import runpy
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]
CLASSES = {"unverified", "development-ad-hoc", "development-signed", "developer-id-notarized"}
schema = json.loads((ROOT / "schemas/windows-hvf-3d-off-product-e2e-receipt-v1.json").read_text())
assert set(schema["properties"]["artifact_signing_class"]["enum"]) == CLASSES
verify = runpy.run_path(str(ROOT / "scripts/verify-windows-product-e2e-receipt.py"))
for kind in CLASSES - {"developer-id-notarized"}:
    receipt = verify["_fixture"]()
    receipt.update(artifact_signing_class=kind, claim_eligible=False)
    verify["validate"](receipt)
    for require_claim, claimed in ((True, False), (False, True)):
        receipt["claim_eligible"] = claimed
        try:
            verify["validate"](receipt, require_claim_eligible=require_claim)
        except verify["ReceiptError"]:
            pass
        else:
            raise AssertionError(f"{kind} became claim eligible")

cases = [
    ("Signature=adhoc", 0, 0, 1, "development-ad-hoc"),
    ("Authority=Apple Development: Fixture", 0, 0, 0, "development-signed"),
    ("Authority=Developer ID Application: Fixture", 0, 0, 0, "developer-id-notarized"),
    ("Authority=Developer ID Application: Fixture", 0, 0, 1, "development-signed"),
    ("Signature=adhoc", 1, 0, 0, None),
    ("Signature=adhoc", 0, 1, 0, None),
    ("unrecognized metadata", 0, 0, 0, None),
]
with tempfile.TemporaryDirectory(prefix="bridgevm-signing-class-") as directory:
    root = Path(directory)
    signer = root / "codesign"
    signer.write_text('#!/bin/sh\nif [ "$1" = "--verify" ]; then exit "$MOCK_VERIFY"; fi\n'
                      '[ "$MOCK_METADATA_STATUS" = 0 ] || exit 1\nprintf "%s\\n" "$MOCK_METADATA"\n')
    assessor = root / "spctl"
    assessor.write_text('#!/bin/sh\nexit "$MOCK_ASSESS"\n')
    signer.chmod(0o755)
    assessor.chmod(0o755)
    for metadata, valid, readable, assessment, expected in cases:
        env = dict(os.environ, PATH=str(root) + os.pathsep + os.environ["PATH"],
                   MOCK_METADATA=metadata, MOCK_VERIFY=str(valid),
                   MOCK_METADATA_STATUS=str(readable), MOCK_ASSESS=str(assessment))
        result = subprocess.run(["bash", str(ROOT / "scripts/live-gates/classify-product-e2e-signing.sh"),
                                 str(root / "fixture app")], env=env, capture_output=True, text=True, timeout=10)
        assert (result.returncode == 0) == (expected is not None), (metadata, result.stderr)
        assert result.stdout.strip() == (expected or ""), metadata
tier = (ROOT / "scripts/live-gates/run-windows-product-e2e-tier.sh").read_text()
assert 'classify-product-e2e-signing.sh" "$APP"' in tier
assert "SIGNING=unverified" in tier
print(f"PASS: {len(cases)} signing classifier cases and three non-promoting receipt classes")
