#!/usr/bin/env python3
"""Reject malformed public T22 fields before a receipt can be published."""
from __future__ import annotations

import json
from pathlib import Path
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/live-gates"))
import a19_interrupted_restore_receipt as receipt

COMMIT = "a" * 40
SHA_A = "a" * 64
SHA_B = "b" * 64


def complete() -> dict:
    value = receipt.initial("public-boundary", COMMIT)
    value.update({field: SHA_A for field in receipt.HASHES})
    value.update({field: True for field in receipt.FLAGS[:8]})
    value.update({"pass": True, "outcome": "completed", "interruption_stage": "staged-disk-verify-read",
                  "boots_attempted": 4, "boots_passed": 4, "natural_shutdown_count": 4,
                  "interruption_case_count": 1, "sample_count": 1, "run_count": 1,
                  "clobber_marker_sha256": SHA_B, "postkill_marker_sha256": SHA_B,
                  "host_model": "Mac17,9", "macos_version": "27.0",
                  "finished_at": value["started_at"]})
    return receipt.validate(value, COMMIT)


class PublicReceiptContract(unittest.TestCase):
    def test_passing_host_and_time_fields_are_bounded(self):
        value = complete()
        bad_fields = (
            ("host_model", ["arbitrary"]), ("host_model", {"path": "/private/media"}),
            ("host_model", "/Users/example/private"), ("host_model", "Mac" + "a" * 100),
            ("host_model", "absent"), ("macos_version", ["27.0"]),
            ("macos_version", "absent"), ("macos_version", "27.0/private"),
            ("started_at", ["2026-09-25T00:00:00Z"]), ("started_at", "absent"),
            ("started_at", "2026-13-25T00:00:00Z"), ("finished_at", "absent"),
            ("finished_at", "2026-09-24T00:00:00Z"),
        )
        for field, bad in bad_fields:
            with self.subTest(field=field, bad=bad), self.assertRaises(ValueError):
                receipt.validate({**value, field: bad}, COMMIT)

    def test_missing_receipt_allows_absent_host_only_with_complete_times(self):
        value = receipt.initial("public-boundary", COMMIT)
        value["finished_at"] = value["started_at"]
        self.assertFalse(receipt.validate(value, COMMIT)["pass"])
        with self.assertRaises(ValueError):
            receipt.validate({**value, "finished_at": "absent"}, COMMIT)
        with self.assertRaises(ValueError):
            receipt.validate({**value, "host_model": ["arbitrary"]}, COMMIT)

    def test_json_loader_rejects_repeated_keys_and_nonfinite_values(self):
        value = complete()
        encoded = json.dumps(value)
        variants = (
            '{"host_model":"Mac17,9",' + encoded[1:],
            '{"nested":{"same":1,"same":2},' + encoded[1:],
            encoded.replace('"host_model": "Mac17,9"', '"host_model": NaN'),
        )
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "receipt.json"
            for payload in variants:
                with self.subTest(payload=payload[:45]), self.assertRaises(ValueError):
                    path.write_text(payload, encoding="utf-8")
                    receipt.load_receipt(path)
            path.write_text(encoded, encoding="utf-8")
            self.assertEqual(receipt.load_receipt(path), value)


if __name__ == "__main__":
    unittest.main()
