#!/usr/bin/env python3
"""The T19 writer carries `bitlocker_restart_check` beside a lane without judging or publishing it;
the T17 writer still refuses it (docs/history/windows-hvf/bitlocker-restart-check-20261023.md)."""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import tempfile
import unittest

import product_e2e_identity_fixtures as fixture
import product_e2e_private_lane as SHARED
import t19_private_lane as T19_PRIVATE

BOOT = {"boot": "enable", "ready": True, "status": "available", "reason": "none", "edition": "pro",
        "conversion": "encrypting", "encryption_percent": 3, "protection": "off", "lock": "unlocked",
        "protectors": ["tpm"], "pcr_profile": [7, 11], "fve_code": None, "pcr_read": "ok",
        "pcr0_sha256": "e" * 64, "pcr7_sha256": "f" * 64}
CHECK = {"schema_version": "bridgevm.windows-product-e2e-bitlocker-restart-check.v1", "outcome": "unlocked",
         "reason": "none", "boots": [BOOT, {**BOOT, "boot": "unlock", "conversion": "encrypted", "encryption_percent": 100}],
         "guest_restart_observed": True, "power_cycle_after_restart": True, "pcr0_equal": True, "pcr7_equal": True,
         "vm_stop": "guest-shutdown"}
RECOVERY = {**CHECK, "outcome": "recovery-suspected", "reason": "unlock-boot-not-ready", "pcr0_equal": None}
# The check never decides a lane, so even a malformed record must not change one.
EVIDENCE = (CHECK, RECOVERY, {"outcome": "malformed"}, None, 7)


class BitLockerPrivateLaneTest(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.receipts = 0

    def tearDown(self):
        self.temporary.cleanup()

    def check_lane(self, writer, path: Path):
        if writer is fixture.T17:
            return writer.lane(path, job_id=fixture.JOB, commit=fixture.COMMIT, mode="pilot", ordinal=1, stamp=None)
        return writer.lane(path, fixture.JOB, fixture.COMMIT, "pilot", 1, None)

    def receipt(self, writer, lane: dict) -> dict:
        self.receipts += 1
        private = self.root / f"private-{self.receipts}"
        private.mkdir()
        result = private / "lane-1-result.json"
        result.write_text(json.dumps(lane))
        (private / "lane-1-authenticated.json").write_text(json.dumps(fixture.stamp(writer, result)))
        manifest = self.root / "manifest.tsv"
        manifest.write_text("campaign_mode\tpilot\nbitlocker_restart_check\ttpm-only\n")
        args = argparse.Namespace(private=private, input_manifest=manifest, verified=self.root / "absent.json",
                                  job_id=fixture.JOB, commit=fixture.COMMIT, mode="pilot", attempts=1,
                                  started_at="2026-10-23T00:00:00Z", outcome="completed", failure_code="none",
                                  signing_class="unverified", cleanup=True, valid=True)
        receipt = writer.build(args)
        for field in ("finished_at", "elapsed_ms"):
            receipt.pop(field)
        return receipt

    def test_only_the_t19_key_set_gains_the_field(self):
        self.assertEqual(SHARED.KEYS, frozenset({"tpm_evidence"}))
        self.assertEqual(T19_PRIVATE.KEYS, frozenset({"tpm_evidence", "bitlocker_restart_check"}))

    def test_t19_lanes_with_the_check_authenticate_like_lanes_without(self):
        base, path = fixture.lane(fixture.T19), self.root / "t19.json"
        for evidence in EVIDENCE:
            for extra in ({}, {"tpm_evidence": {"boots": []}}):
                with self.subTest(evidence=evidence, extra=sorted(extra)):
                    path.write_text(json.dumps({**base, **extra, "bitlocker_restart_check": evidence}))
                    value = self.check_lane(fixture.T19, path)
                    self.assertEqual({key: item for key, item in value.items() if key not in T19_PRIVATE.KEYS}, base)

    def test_the_check_never_rescues_an_inconsistent_t19_lane(self):
        base, path = fixture.lane(fixture.T19), self.root / "t19.json"
        failed = {**base, fixture.T19.LANE_STAGES[-1]: False}
        for evidence in EVIDENCE:
            with self.subTest(evidence=evidence):
                path.write_text(json.dumps({**failed, "bitlocker_restart_check": evidence}))
                with self.assertRaises(ValueError):
                    self.check_lane(fixture.T19, path)

    def test_the_t17_writer_refuses_the_field(self):
        base, path = fixture.lane(fixture.T17), self.root / "t17.json"
        for evidence in EVIDENCE:
            with self.subTest(evidence=evidence):
                path.write_text(json.dumps({**base, "bitlocker_restart_check": evidence}))
                with self.assertRaises(ValueError):
                    self.check_lane(fixture.T17, path)

    def test_other_unknown_t19_fields_are_still_refused(self):
        base, path = fixture.lane(fixture.T19), self.root / "t19.json"
        for extra in ({"bitlocker": CHECK}, {"bitlocker_restart_check_v2": CHECK}, {"bitlocker_restart_check": CHECK, "recovery": True}):
            with self.subTest(extra=sorted(extra)):
                path.write_text(json.dumps({**base, **extra}))
                with self.assertRaises(ValueError):
                    self.check_lane(fixture.T19, path)

    def test_public_t19_receipt_is_unchanged_by_the_check(self):
        base = fixture.lane(fixture.T19)
        plain = self.receipt(fixture.T19, base)
        self.assertEqual(plain["passes"], 1)
        for evidence in EVIDENCE:
            with self.subTest(evidence=evidence):
                receipt = self.receipt(fixture.T19, {**base, "bitlocker_restart_check": evidence})
                self.assertEqual(receipt, plain)
                text = json.dumps(receipt).lower()
                for marker in ("bitlocker", "recovery-suspected", "protector", "pcr"):
                    self.assertNotIn(marker, text)


if __name__ == "__main__":
    result = unittest.main(exit=False).result
    raise SystemExit(0 if result.wasSuccessful() else 1)
