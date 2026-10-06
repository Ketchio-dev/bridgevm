#!/usr/bin/env python3
"""Reject ambiguous or unsafe JSON before A19 export bytes become boot evidence."""
from __future__ import annotations

import hashlib
import importlib.util
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/live-gates"))
import native_snapshot_export_evidence as evidence  # noqa: E402
import native_snapshot_export_json as metadata  # noqa: E402

SPEC = importlib.util.spec_from_file_location(
    "export_fixture", ROOT / "tests/integration/native-snapshot-export-evidence-contract.py")
FIXTURE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(FIXTURE)


class NativeSnapshotExportJSONContract(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.result, self.export, self.library, self.vm_id = (
            FIXTURE.NativeSnapshotExportEvidenceContract().fixture(self.root))

    def tearDown(self):
        self.temporary.cleanup()

    def verify(self):
        return evidence.verify(self.result, self.export, self.vm_id, self.library)

    def test_success_and_format_version_require_exact_json_types(self):
        for path, field, invalid in ((self.result, "complete", (1, 1.0, "true")),
                                     (self.export / "manifest.json", "format_version", (True, 1.0, "1"))):
            original = path.read_bytes()
            for value in invalid:
                with self.subTest(field=field, value=value):
                    edited = json.loads(original)
                    edited[field] = value
                    path.write_text(json.dumps(edited))
                    with self.assertRaises(ValueError):
                        self.verify()
            path.write_bytes(original)

    def test_duplicate_fields_are_rejected_in_all_metadata(self):
        retained = self.root / "export-evidence.json"
        retained.write_text(json.dumps(self.verify()))
        specs = ((self.result, "complete", "false", self.verify),
                 (self.export / "manifest.json", "disk_bytes", "0", self.verify),
                 (retained, "disk_sha256", '"' + "0" * 64 + '"',
                  lambda: evidence.load_evidence(retained, self.vm_id)))
        for path, field, wrong, read in specs:
            original = path.read_bytes()
            with self.subTest(path=path.name):
                # The final valid field must not hide the preceding invalid field.
                path.write_bytes(b'{"' + field.encode() + b'":' + wrong.encode() + b',' + original[1:])
                with self.assertRaises(ValueError):
                    read()
            path.write_bytes(original)

    def test_nonfinite_fields_cannot_be_overwritten_by_valid_duplicates(self):
        original = self.result.read_bytes()
        for constant in (b"NaN", b"Infinity", b"-Infinity"):
            with self.subTest(constant=constant):
                self.result.write_bytes(b'{"complete":' + constant + b',' + original[1:])
                with self.assertRaises(ValueError):
                    self.verify()

    def test_metadata_is_bounded_regular_and_never_follows_a_symlink(self):
        original = self.result.read_bytes()
        for mutation in ("empty", "oversized", "symlink", "fifo"):
            with self.subTest(mutation=mutation):
                self.result.unlink()
                if mutation == "empty":
                    self.result.write_bytes(b"")
                elif mutation == "oversized":
                    self.result.write_bytes(original + b" " * metadata.LIMIT)
                elif mutation == "symlink":
                    target = self.root / "actual-result.json"
                    target.write_bytes(original)
                    self.result.symlink_to(target)
                else:
                    os.mkfifo(self.result)
                with self.assertRaises((OSError, ValueError)):
                    self.verify()
                self.result.unlink()
                self.result.write_bytes(original)

    def test_result_hash_binds_the_checked_bytes(self):
        original = self.result.read_bytes()
        original_read = metadata.read_bounded_regular

        def read_then_replace(path, limit):
            data = original_read(path, limit)
            if path == self.result:
                self.result.write_bytes(b"replaced after the checked read")
            return data

        with mock.patch.object(metadata, "read_bounded_regular", side_effect=read_then_replace):
            value = self.verify()
        self.assertEqual(value["result_sha256"], hashlib.sha256(original).hexdigest())
        self.assertNotEqual(value["result_sha256"], hashlib.sha256(self.result.read_bytes()).hexdigest())


if __name__ == "__main__":
    unittest.main()
