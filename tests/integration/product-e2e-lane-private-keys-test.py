#!/usr/bin/env python3
"""A T17 or T19 lane result may carry both private keys at once.

`tpm_evidence` (product_e2e_private_lane.py) is never judged, and
`first_run_host_records` (product_e2e_host_records.py) is admitted only well
formed. Both receipt writers authenticate a lane holding both like the plain
lane, still refuse malformed host records and any other unknown key beside
them, and never copy either key into a receipt. ProductE2EPrivateLaneCompositionTests.swift
pins the same nesting on the Swift side.
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys
import tempfile
import unittest

import product_e2e_identity_fixtures as fixture

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/live-gates"))
import product_e2e_host_records as HOST_RECORDS  # noqa: E402

TPM = {"schema_version": "bridgevm.windows-product-e2e-tpm-evidence.v1", "boots": []}
RECORDS = {name: {field: 1 for field in sorted(fields)} for name, fields in HOST_RECORDS.LANE_RECORDS.items()}
BOTH = {"tpm_evidence": TPM, HOST_RECORDS.KEY: RECORDS}


class PrivateKeysComposition(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.count = 0

    def tearDown(self):
        self.temporary.cleanup()

    def write(self, lane: dict) -> Path:
        self.count += 1
        path = self.root / f"lane-{self.count}.json"
        path.write_text(json.dumps(lane))
        return path

    def check_lane(self, writer, lane: dict) -> dict:
        path = self.write(lane)
        if writer is fixture.T17:
            return writer.lane(path, job_id=fixture.JOB, commit=fixture.COMMIT, mode="pilot", ordinal=1)
        return writer.lane(path, fixture.JOB, fixture.COMMIT, "pilot", 1)

    def receipt(self, writer, lane: dict) -> dict:
        private = self.root / f"private-{self.count + 1}"
        private.mkdir()
        result = private / "lane-1-result.json"
        result.write_text(json.dumps(lane))
        (private / "lane-1-authenticated.json").write_text(json.dumps(fixture.stamp(writer, result)))
        manifest = self.root / "manifest.tsv"
        manifest.write_text("campaign_mode\tpilot\n")
        self.count += 1
        args = argparse.Namespace(private=private, input_manifest=manifest, verified=self.root / "absent.json",
                                  job_id=fixture.JOB, commit=fixture.COMMIT, mode="pilot", attempts=1,
                                  started_at="2026-10-23T00:00:00Z", outcome="completed", failure_code="none",
                                  signing_class="unverified", cleanup=True, valid=True)
        receipt = writer.build(args)
        for field in ("finished_at", "elapsed_ms"):
            receipt.pop(field)
        return receipt

    def test_a_lane_with_both_private_keys_authenticates_like_the_plain_lane(self):
        self.assertTrue(RECORDS)
        for writer in (fixture.T17, fixture.T19):
            base = fixture.lane(writer)
            with self.subTest(tier=writer.__name__):
                value = self.check_lane(writer, {**base, **BOTH})
                self.assertEqual({key: item for key, item in value.items() if key not in BOTH}, base)

    def test_host_records_stay_validated_beside_tpm_evidence(self):
        malformed = ({"hda_poll": {}}, {"unknown_record": {"polls": 1}}, {}, "records")
        for writer in (fixture.T17, fixture.T19):
            base = fixture.lane(writer)
            for records in malformed:
                with self.subTest(tier=writer.__name__, records=records):
                    with self.assertRaises(ValueError):
                        self.check_lane(writer, {**base, "tpm_evidence": TPM, HOST_RECORDS.KEY: records})

    def test_any_other_unknown_key_beside_both_is_refused(self):
        for writer in (fixture.T17, fixture.T19):
            base = fixture.lane(writer)
            for extra in ({"pcr0_sha256": "e" * 64}, {"hda_poll": RECORDS["hda_poll"]}, {"tpm": TPM}):
                with self.subTest(tier=writer.__name__, extra=sorted(extra)):
                    with self.assertRaises(ValueError):
                        self.check_lane(writer, {**base, **BOTH, **extra})

    def test_no_receipt_copies_either_key(self):
        for writer in (fixture.T17, fixture.T19):
            base = fixture.lane(writer)
            with self.subTest(tier=writer.__name__):
                plain = self.receipt(writer, base)
                self.assertEqual(plain["passes"], 1)
                receipt = self.receipt(writer, {**base, **BOTH})
                self.assertEqual(receipt, plain)
                text = json.dumps(receipt)
                for marker in ("tpm_evidence", HOST_RECORDS.KEY, "hda_", "pcr"):
                    self.assertNotIn(marker, text)


if __name__ == "__main__":
    result = unittest.main(exit=False).result
    raise SystemExit(0 if result.wasSuccessful() else 1)
