#!/usr/bin/env python3
"""T17/T19 lane writers carry private TPM evidence without judging or publishing it."""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import tempfile
import unittest

import product_e2e_identity_fixtures as fixture

BOOT = {"boot": 1, "status": "available", "reason": "none", "tpm_present": True, "tpm_ready": True,
        "pcr0_sha256": "e" * 64, "pcr7_sha256": "f" * 64, "pcr0_equals_first_boot": True,
        "guest_restart_before_read": True}
TPM = {"schema_version": "bridgevm.windows-product-e2e-tpm-evidence.v1",
       "boots": [BOOT, {**BOOT, "boot": 2, "guest_restart_before_read": False},
                 {"boot": 3, "status": "unavailable", "reason": "result-missing", "tpm_present": None,
                  "tpm_ready": None, "pcr0_sha256": None, "pcr7_sha256": None,
                  "pcr0_equals_first_boot": None, "guest_restart_before_read": False}]}
# Diagnostics never decide a lane, so even a malformed record must not change one.
EVIDENCE = (TPM, {"boots": "malformed"}, None, 7)


class PrivateLaneContract(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.receipts = 0

    def tearDown(self):
        self.temporary.cleanup()

    def check_lane(self, writer, path: Path, stamp: Path | None = None):
        if writer is fixture.T17:
            return writer.lane(path, job_id=fixture.JOB, commit=fixture.COMMIT, mode="pilot", ordinal=1, stamp=stamp)
        return writer.lane(path, fixture.JOB, fixture.COMMIT, "pilot", 1, stamp)

    def receipt(self, writer, lane: dict) -> dict:
        self.receipts += 1
        private = self.root / f"private-{self.receipts}"
        private.mkdir()
        result = private / "lane-1-result.json"
        result.write_text(json.dumps(lane))
        (private / "lane-1-authenticated.json").write_text(json.dumps(fixture.stamp(writer, result)))
        manifest = self.root / "manifest.tsv"
        manifest.write_text("campaign_mode\tpilot\n")
        args = argparse.Namespace(private=private, input_manifest=manifest, verified=self.root / "absent.json",
                                  job_id=fixture.JOB, commit=fixture.COMMIT, mode="pilot", attempts=1,
                                  started_at="2026-10-23T00:00:00Z", outcome="completed", failure_code="none",
                                  signing_class="unverified", cleanup=True, valid=True)
        receipt = writer.build(args)
        for field in ("finished_at", "elapsed_ms"):
            receipt.pop(field)
        return receipt

    def test_lanes_with_private_tpm_evidence_authenticate_like_lanes_without(self):
        for writer in (fixture.T17, fixture.T19):
            base, path = fixture.lane(writer), self.root / f"{writer.__name__}.json"
            for evidence in EVIDENCE:
                with self.subTest(tier=writer.__name__, evidence=evidence):
                    path.write_text(json.dumps({**base, "tpm_evidence": evidence}))
                    value = self.check_lane(writer, path)
                    self.assertEqual({key: item for key, item in value.items() if key != "tpm_evidence"}, base)

    def test_private_evidence_never_rescues_an_inconsistent_lane(self):
        for writer in (fixture.T17, fixture.T19):
            base, path = fixture.lane(writer), self.root / f"{writer.__name__}.json"
            failed = {**base, writer.LANE_STAGES[-1]: False}
            for evidence in EVIDENCE:
                with self.subTest(tier=writer.__name__, evidence=evidence):
                    path.write_text(json.dumps({**failed, "tpm_evidence": evidence}))
                    with self.assertRaises(ValueError):
                        self.check_lane(writer, path)

    def test_other_unknown_lane_fields_are_still_refused(self):
        for writer in (fixture.T17, fixture.T19):
            base, path = fixture.lane(writer), self.root / f"{writer.__name__}.json"
            for extra in ({"tpm": TPM}, {"pcr0_sha256": "e" * 64}, {"tpm_evidence": TPM, "pcr7_sha256": "f" * 64}):
                with self.subTest(tier=writer.__name__, extra=sorted(extra)):
                    path.write_text(json.dumps({**base, **extra}))
                    with self.assertRaises(ValueError):
                        self.check_lane(writer, path)

    def test_public_receipt_is_unchanged_by_private_tpm_evidence(self):
        for writer in (fixture.T17, fixture.T19):
            base = fixture.lane(writer)
            plain = self.receipt(writer, base)
            self.assertEqual(plain["passes"], 1)
            for evidence in EVIDENCE:
                with self.subTest(tier=writer.__name__, evidence=evidence):
                    receipt = self.receipt(writer, {**base, "tpm_evidence": evidence})
                    self.assertEqual(receipt, plain)
                    text = json.dumps(receipt)
                    self.assertNotIn("tpm_evidence", text)
                    self.assertNotIn("pcr", text)


if __name__ == "__main__":
    result = unittest.main(exit=False).result
    raise SystemExit(0 if result.wasSuccessful() else 1)
