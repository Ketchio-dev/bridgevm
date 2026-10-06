#!/usr/bin/env python3
"""T17/T19 identities keep JSON types; T19 audit seals the prelaunch request bytes."""
from __future__ import annotations

import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import product_e2e_identity_fixtures as fixture


class FixedIdentityContract(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)

    def tearDown(self):
        self.temporary.cleanup()

    def check_lane(self, writer, path: Path, stamp: Path | None = None):
        if writer is fixture.T17:
            return writer.lane(path, job_id=fixture.JOB, commit=fixture.COMMIT,
                               mode="pilot", ordinal=1, stamp=stamp)
        return writer.lane(path, fixture.JOB, fixture.COMMIT, "pilot", 1, stamp)

    def test_lanes_refuse_boolean_ordinals_and_numeric_three_d_flags(self):
        for writer in (fixture.T17, fixture.T19):
            path = self.root / f"{writer.__name__}.json"
            original = fixture.lane(writer)
            path.write_text(json.dumps(original))
            self.assertEqual(self.check_lane(writer, path), original)
            for field, invalid in (("lane", True), ("lane", 1.0),
                                   ("three_d_injection", 0), ("three_d_injection", 0.0)):
                with self.subTest(tier=writer.__name__, field=field, invalid=invalid):
                    path.write_text(json.dumps({**original, field: invalid}))
                    with self.assertRaises(ValueError):
                        self.check_lane(writer, path)

    def test_stamps_refuse_boolean_ordinals_and_nonstring_request_hashes(self):
        for writer in (fixture.T17, fixture.T19):
            path, stamp = self.root / f"{writer.__name__}.json", self.root / "stamp.json"
            path.write_text(json.dumps(fixture.lane(writer)))
            original = fixture.stamp(writer, path)
            stamp.write_text(json.dumps(original))
            self.check_lane(writer, path, stamp)
            for field, invalid in (("lane", True), ("lane", 1.0), ("request_sha256", int("1" * 64))):
                with self.subTest(tier=writer.__name__, field=field, invalid=invalid):
                    stamp.write_text(json.dumps({**original, field: invalid}))
                    with self.assertRaises(ValueError):
                        self.check_lane(writer, path, stamp)

    def test_t17_requests_refuse_coerced_identity_before_artifact_authentication(self):
        writer = fixture.T17
        request_path, result_path = self.root / "request.json", self.root / "result.json"
        result_path.write_text(json.dumps(fixture.lane(writer)))
        prefix = fixture.NONCE[:12]
        request = {"schema_version": "bridgevm.windows-hvf-3d-off-product-e2e-request.v2",
                   "job_id": fixture.JOB, "commit": fixture.COMMIT, "campaign_mode": "pilot",
                   "lane": 1, "nonce": fixture.NONCE, "three_d_injection": False,
                   "vm_name": f"BridgeVM T17 Lane 1 {prefix}", "vm_slug": f"bridgevm-t17-lane-1-{prefix}",
                   **{field: f"/private/tmp/fixture/{field}" for field in writer.REQUEST_PATHS}}
        for field, invalid in ((None, None), ("lane", True), ("lane", 1.0), ("three_d_injection", 0)):
            stamp = self.root / f"stamp-{field}-{invalid}.json"
            request_path.write_text(json.dumps(request if field is None else {**request, field: invalid}))
            with self.subTest(field=field, invalid=invalid), patch.object(writer.ARTIFACTS, "authenticate") as artifacts:
                if field is None:
                    writer.authenticate(request_path, result_path, stamp, job_id=fixture.JOB,
                                        commit=fixture.COMMIT, mode="pilot", ordinal=1)
                    artifacts.assert_called_once()
                else:
                    with self.assertRaises(ValueError):
                        writer.authenticate(request_path, result_path, stamp, job_id=fixture.JOB,
                                            commit=fixture.COMMIT, mode="pilot", ordinal=1)
                    artifacts.assert_not_called()
                    self.assertFalse(stamp.exists())


class ImportRequestSealContract(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="bridgevm-import-e2e-integrity-", dir="/private/tmp")
        self.root = Path(self.temporary.name) / "lane-1"
        self.request, self.result = fixture.import_request(self.root)
        self.prelaunch_hash = fixture.T19.digest(self.request)
        self.stamp = self.root.parent / "stamp.json"

    def tearDown(self):
        self.temporary.cleanup()

    def audit(self, expected_hash: str | None = None):
        fixture.T19.authenticate(self.request, self.result, self.stamp, fixture.JOB, fixture.COMMIT,
                                 "pilot", 1, self.prelaunch_hash if expected_hash is None else expected_hash)

    def command(self, include_hash: bool = True) -> subprocess.CompletedProcess:
        command = [sys.executable, str(fixture.ROOT / "scripts/live-gates/write-windows-import-product-e2e-receipt.py"),
                   "--check-lane", str(self.result), "--request", str(self.request), "--stamp", str(self.stamp),
                   "--job-id", fixture.JOB, "--commit", fixture.COMMIT, "--mode", "pilot", "--ordinal", "1"]
        if include_hash:
            command += ["--expected-request-sha256", self.prelaunch_hash]
        return subprocess.run(command, capture_output=True, text=True, timeout=30)

    def test_original_request_authenticates_and_cli_records_its_seal(self):
        self.audit()
        self.assertEqual(json.loads(self.stamp.read_text())["request_sha256"], self.prelaunch_hash)
        self.stamp.unlink()
        completed = self.command()
        self.assertEqual(completed.returncode, 0, completed.stderr)
        self.assertEqual(json.loads(self.stamp.read_text())["request_sha256"], self.prelaunch_hash)

    def test_byte_append_and_path_rewrites_refuse_the_original_seal(self):
        original = self.request.read_bytes()
        for field in (None, "app_bundle_path", "source_vtpm_package_path", "source_vtpm_code_path"):
            with self.subTest(field=field):
                if field is None:
                    self.request.write_bytes(original + b" ")
                else:
                    request = json.loads(original)
                    request[field] = "/private/tmp/unrequested-resource"
                    self.request.write_text(json.dumps(request))
                with self.assertRaisesRegex(ValueError, "prelaunch SHA-256"):
                    self.audit()
                completed = self.command()
                self.assertEqual(completed.returncode, 1)
                self.assertIn("prelaunch SHA-256", completed.stderr)
                self.assertFalse(self.stamp.exists())
        self.assertTrue(self.result.is_file())  # The rejected result is preserved for failure analysis.

    def test_request_audit_requires_a_valid_external_prelaunch_hash(self):
        self.assertEqual(self.command(include_hash=False).returncode, 2)
        for invalid in ("", "absent", "A" * 64, int("1" * 64), "f" * 64):
            with self.subTest(invalid=invalid), self.assertRaises(ValueError):
                self.audit(invalid)
        self.assertFalse(self.stamp.exists())

    def test_t19_requests_refuse_coercions_even_when_their_bytes_match_the_seal(self):
        original = self.request.read_bytes()
        for field, invalid in (("lane", True), ("lane", 1.0), ("three_d_injection", 0),
                               ("three_d_injection", 0.0), ("source_vtpm_code_path", False)):
            with self.subTest(field=field, invalid=invalid):
                request = json.loads(original)
                request[field] = invalid
                self.request.write_text(json.dumps(request))
                with self.assertRaises(ValueError):
                    self.audit(fixture.T19.digest(self.request))
                self.assertFalse(self.stamp.exists())

    def test_recovery_paths_must_belong_to_the_exact_lane_even_with_a_matching_seal(self):
        original = self.request.read_bytes()
        for field in ("source_vtpm_code_path", "source_vtpm_package_path"):
            with self.subTest(field=field):
                request = json.loads(original)
                request[field] = "/private/tmp/unrequested-resource"
                self.request.write_text(json.dumps(request))
                with self.assertRaisesRegex(ValueError, "fixed root"):
                    self.audit(fixture.T19.digest(self.request))
                self.assertFalse(self.stamp.exists())

if __name__ == "__main__":
    unittest.main()
