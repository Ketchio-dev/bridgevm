#!/usr/bin/env python3
"""Actual CLI read authenticates archived T21/T22 receipts before any output."""
from __future__ import annotations

import json
from pathlib import Path
import tempfile
import unittest

from a19_archived_receipt_fixtures import fixture, interrupt, quota, read
from a19_archived_receipt_hint_cases import ArchivedHintCases
from a19_archived_receipt_refusal_cases import ArchivedRefusalCases
from a19_interrupt_case_fixtures import SHA_C, SHA_D, passing, proven
import a19_archived_receipt_read as archive
from a19_lifecycle_campaign_read import strict_reader as lifecycle_reader

class ArchivedReceiptReadContract(ArchivedHintCases, ArchivedRefusalCases, unittest.TestCase):
    def refused(self, root: Path, job: Path):
        result = read(root, job)
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertEqual(result.stdout, b"")

    def test_valid_historical_t21_and_t22_archives_remain_readable(self):
        for number in (21, 22):
            with self.subTest(tier=number), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                job, value = fixture(root, number)
                result = read(root, job)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(json.loads(result.stdout), value)
                self.assertFalse(value["claim_eligible"])

    def test_schema_2_partial_and_three_case_archives_preserve_their_counts(self):
        for additions in ({}, proven("swap_", SHA_C), {**proven("swap_", SHA_C), **proven("create_", SHA_D)}):
            count = 1 + sum(name.endswith("interruption_stage") for name in additions)
            with self.subTest(count=count), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                value = passing(**additions, interruption_case_count=count)
                job, _ = fixture(root, 22, value)
                result = read(root, job)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(json.loads(result.stdout), value)
                self.assertEqual(value["sample_count"], 1)
                self.assertFalse(value["claim_eligible"])

    def test_failed_but_clean_sealed_archives_are_readable_not_promoted(self):
        for number, contract in ((21, quota), (22, interrupt)):
            with self.subTest(tier=number), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                value = contract.initial("failed-archive", "a" * 40)
                value.update(finished_at=value["started_at"], worker_cleanup_verified=True,
                             input_manifest_sha256="b" * 64, binary_hash="c" * 64)
                job, _ = fixture(root, number, value)
                result = read(root, job)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(json.loads(result.stdout), value)
                self.assertFalse(value["pass"])
                self.assertFalse(value["criterion_pass"])

    def test_changed_public_result_is_refused_before_any_output(self):
        for number in (21, 22):
            with self.subTest(tier=number), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                job, value = fixture(root, number)
                changed = {**value, "app_artifact_sha256": "1" * 64}
                # A valid schema is not retained-original correspondence.
                (quota if number == 21 else interrupt).validate(changed, value["commit"])
                (job / "receipt.public.json").write_text(json.dumps(changed) + "\n")
                self.refused(root, job)

    def test_reader_dispatch_preserves_existing_strict_and_legacy_routes(self):
        for tier in ("t20-a19-native-snapshot-restore", "t23-a19-lifecycle-campaign"):
            self.assertIs(archive.strict_reader((None, tier, None)), lifecycle_reader((tier,)))
        for tier, reader in ((quota.TIER, archive.read_quota), (interrupt.TIER, archive.read_interrupt)):
            for hints in ((tier, None, None), (None, tier, None), (None, None, tier)):
                self.assertIs(archive.strict_reader(hints), reader)
        self.assertIsNone(archive.strict_reader(("t8-pointer-reliability", None, ["unhashable"])))


if __name__ == "__main__":
    unittest.main()
