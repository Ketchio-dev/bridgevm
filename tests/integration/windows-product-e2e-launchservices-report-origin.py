#!/usr/bin/env python3
"""Reject contradictory T17 LaunchServices reports without touching TCC."""
import hashlib
import importlib.util
import json
import subprocess
import unittest
from pathlib import Path
from unittest import mock

SOURCE = Path(__file__).with_name("windows-product-e2e-launchservices-admission.py")
spec = importlib.util.spec_from_file_location("t17_admission_contract", SOURCE)
assert spec and spec.loader
contract = importlib.util.module_from_spec(spec)
spec.loader.exec_module(contract)


class ReportOriginContract(unittest.TestCase):
    def setUp(self):
        self.fixture = contract.AdmissionContract(
            "test_mocked_authenticated_observation_is_only_success_path"
        )
        self.fixture.setUp()
        self.addCleanup(self.fixture.doCleanups)
        self.path_hash = hashlib.sha256(
            str(self.fixture.helper_app.resolve()).encode("utf-8")
        ).hexdigest()

    def invoke(self, path_hash: str, open_status: int):
        report = {
            "schema": "t17.accessibility-diagnostic.v1",
            "observation_only": True,
            "criterion_pass": False,
            "accessibility_trusted": True,
            "scope": "calling-process-only-not-product-e2e-or-tcc-database-attribution",
            "caller_identity": {
                "schema": "t17.caller-identity.v1",
                "pid": "42", "ppid": "1",
                "bundle_id": "dev.bridgevm.product-e2e",
                "bundle_path_sha256": path_hash,
                "executable_name": "BridgeVMProductE2E",
                "scope": "on-disk-code-metadata-not-signature-validation-or-tcc-attribution",
                "caller_status": "0", "static_code_status": "0",
                "signing_status": "0",
                "code_identifier": "dev.bridgevm.product-e2e",
                "code_cdhash": "b" * 40,
            },
        }

        def observe(command, **_kwargs):
            self.assertEqual(command[:3], [
                "/usr/bin/open", "-n", str(self.fixture.helper_app)
            ])
            output = Path(command[command.index("--stdout") + 1])
            output.write_text(json.dumps(report), encoding="utf-8")
            return subprocess.CompletedProcess(command, open_status)

        with mock.patch.object(contract.admission.subprocess, "run", side_effect=observe):
            return self.fixture.invoke()

    def test_wrong_helper_path_digest_is_refused(self):
        wrong = ("0" if self.path_hash[0] != "0" else "1") + self.path_hash[1:]
        code, message = self.invoke(wrong, 0)
        self.assertEqual(code, 1)
        self.assertIn("invalid bundle_path_sha256", message)

    def test_nonzero_open_with_valid_report_is_refused(self):
        code, message = self.invoke(self.path_hash, 9)
        self.assertEqual(code, 1)
        self.assertIn("LaunchServices open exited 9", message)


if __name__ == "__main__":
    unittest.main(verbosity=2)
