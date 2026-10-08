#!/usr/bin/env python3
"""Missing-report refusal must not read or disclose helper stderr; no OS launch."""
import importlib.util
from pathlib import Path
import subprocess
import unittest
from unittest import mock

SOURCE = Path(__file__).with_name("windows-product-e2e-launchservices-admission.py")
spec = importlib.util.spec_from_file_location("diagnostic_privacy_fixture", SOURCE)
assert spec and spec.loader
fixture_module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fixture_module)
admission = fixture_module.admission


class DiagnosticPrivacy(unittest.TestCase):
    def check_refusal(self, empty_report):
        fixture = fixture_module.AdmissionContract("test_absent_launchservices_report_is_blocked")
        fixture.setUp()
        self.addCleanup(fixture.doCleanups)
        private = "PRIVATE_FIXTURE /private-fixture/operator-media.iso"
        def launch(command, **_kwargs):
            errors = Path(command[command.index("--stderr") + 1])
            errors.write_text(private)
            if empty_report:
                Path(command[command.index("--stdout") + 1]).touch()
            return subprocess.CompletedProcess(command, 0)
        original = Path.read_text
        observed = []
        def read_text(path, *args, **kwargs):
            if path.name == "launchservices.log":
                observed.append(path.stat().st_size)
            return original(path, *args, **kwargs)
        with mock.patch.object(admission.subprocess, "run", side_effect=launch), \
                mock.patch.object(admission.time, "monotonic", side_effect=[0, 11]), \
                mock.patch.object(Path, "read_text", read_text):
            code, message = fixture.invoke()
        self.assertEqual(code, 1)
        self.assertEqual(message, "T17 submission preflight blocked: LaunchServices diagnostic produced no report\n")
        self.assertNotIn(private, message)
        self.assertEqual(observed, [], "refusal must not open an unbounded diagnostic file")

    def test_absent_report_never_reads_or_echoes_error_content(self):
        self.check_refusal(False)

    def test_empty_report_never_reads_or_echoes_error_content(self):
        self.check_refusal(True)


if __name__ == "__main__":
    unittest.main(verbosity=2)
