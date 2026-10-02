#!/usr/bin/env python3
"""T17/T19 stamps never authenticate request or result bytes changed during audit."""
from __future__ import annotations

import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import product_e2e_identity_fixtures as fixture


class StampMutationContract(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="bridgevm-import-e2e-stamp-", dir="/private/tmp")
        self.root = Path(self.temporary.name)
        self.request, self.result = fixture.import_request(self.root / "lane-1")
        self.stamp = self.root / "stamp.json"
        self.request_seal = fixture.T19.digest(self.request)

    def tearDown(self):
        self.temporary.cleanup()

    def prepare_t17(self):
        writer = fixture.T17
        prefix = fixture.NONCE[:12]
        value = {"schema_version": "bridgevm.windows-hvf-3d-off-product-e2e-request.v2",
                 "job_id": fixture.JOB, "commit": fixture.COMMIT, "campaign_mode": "pilot",
                 "lane": 1, "nonce": fixture.NONCE, "three_d_injection": False,
                 "vm_name": f"BridgeVM T17 Lane 1 {prefix}", "vm_slug": f"bridgevm-t17-lane-1-{prefix}",
                 **{field: f"/private/tmp/fixture/{field}" for field in writer.REQUEST_PATHS}}
        self.request.write_text(json.dumps(value))
        self.result.write_text(json.dumps(fixture.lane(writer)))

    def audit(self, writer):
        if writer is fixture.T17:
            writer.authenticate(self.request, self.result, self.stamp, job_id=fixture.JOB,
                                commit=fixture.COMMIT, mode="pilot", ordinal=1)
        else:
            writer.authenticate(self.request, self.result, self.stamp, fixture.JOB, fixture.COMMIT,
                                "pilot", 1, self.request_seal)

    def assert_mutation_refused(self, writer, target: str, whitespace: bool = False):
        if writer is fixture.T17:
            self.prepare_t17()
            module, function = writer.ARTIFACTS, "authenticate"
            original = lambda *_arguments: None
        else:
            module, function = writer.GUEST, "verify"
            original = module.verify
        path = self.request if target == "request" else self.result

        def verified_then_changed(*arguments):
            value = original(*arguments)
            if whitespace:
                path.write_bytes(path.read_bytes() + b" ")
            else:
                document = json.loads(path.read_text())
                document["app_bundle_path" if target == "request" else "final_disk_sha256"] = (
                    "/private/tmp/unrequested-app" if target == "request" else "e" * 64)
                path.write_text(json.dumps(document))
            return value

        with patch.object(module, function, side_effect=verified_then_changed):
            with self.assertRaisesRegex(ValueError, "changed before stamping"):
                self.audit(writer)
        self.assertFalse(self.stamp.exists())
        self.assertTrue(path.is_file())  # Rejected bytes remain available for failure analysis.

    def test_t19_final_hash_changed_after_guest_verification_is_not_stamped(self):
        self.assert_mutation_refused(fixture.T19, "result")

    def test_t19_request_changed_after_guest_verification_is_not_stamped(self):
        self.assert_mutation_refused(fixture.T19, "request")

    def test_t17_final_hash_changed_after_artifact_verification_is_not_stamped(self):
        self.assert_mutation_refused(fixture.T17, "result")

    def test_t17_request_changed_after_artifact_verification_is_not_stamped(self):
        self.assert_mutation_refused(fixture.T17, "request")

    def test_json_equivalent_result_append_is_not_stamped(self):
        self.assert_mutation_refused(fixture.T19, "result", whitespace=True)

    def test_original_bytes_stamp_and_later_result_mutation_invalidates_stamp(self):
        self.audit(fixture.T19)
        stamp = json.loads(self.stamp.read_text())
        self.assertEqual(stamp["request_sha256"], self.request_seal)
        self.assertEqual(stamp["result_sha256"], fixture.T19.digest(self.result))
        value = json.loads(self.result.read_text()); value["final_disk_sha256"] = "e" * 64
        self.result.write_text(json.dumps(value))
        with self.assertRaisesRegex(ValueError, "stamp is invalid"):
            fixture.T19.lane(self.result, fixture.JOB, fixture.COMMIT, "pilot", 1, self.stamp)


if __name__ == "__main__":
    unittest.main()
