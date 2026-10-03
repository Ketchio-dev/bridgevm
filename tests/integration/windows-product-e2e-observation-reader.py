#!/usr/bin/env python3
"""Owned T17 report-reader and immutable-selector regressions; no OS launch."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest import mock

SOURCE = Path(__file__).with_name("windows-product-e2e-launchservices-admission.py")
spec = importlib.util.spec_from_file_location("t17_reader_fixture", SOURCE)
assert spec and spec.loader
fixture_module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fixture_module)
admission = fixture_module.admission


def report(helper):
    return {
        "schema": "t17.accessibility-diagnostic.v1", "observation_only": True,
        "criterion_pass": False, "accessibility_trusted": True,
        "scope": "calling-process-only-not-product-e2e-or-tcc-database-attribution",
        "caller_identity": {
            "schema": "t17.caller-identity.v1", "pid": "42", "ppid": "1",
            "bundle_id": "dev.bridgevm.product-e2e",
            "bundle_path_sha256": hashlib.sha256(str(helper.resolve()).encode()).hexdigest(),
            "executable_name": "BridgeVMProductE2E",
            "scope": "on-disk-code-metadata-not-signature-validation-or-tcc-attribution",
            "caller_status": "0", "static_code_status": "0", "signing_status": "0",
            "code_identifier": "dev.bridgevm.product-e2e", "code_cdhash": "b" * 40,
        },
    }


class ReaderContracts(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="t17-reader-owned.")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.path = self.root / "report.json"

    def read(self, content):
        self.path.write_bytes(content)
        return admission.read_report(self.path)

    def test_conflicting_or_nested_duplicate_fields_refuse(self):
        for content in [b'{"accessibility_trusted":false,"accessibility_trusted":true}',
                        b'{"identity":{"pid":"1","pid":"2"}}']:
            with self.subTest(content=content), self.assertRaises(admission.PreflightError):
                self.read(content)

    def test_nonfinite_values_refuse(self):
        for value in (b"NaN", b"Infinity", b"-Infinity"):
            with self.subTest(value=value), self.assertRaises(admission.PreflightError):
                self.read(b'{"value":' + value + b'}')

    def test_exact_bound_accepts_and_oversized_or_empty_refuses(self):
        content = b'{"pad":"' + b'x' * (32768 - 10) + b'"}'
        self.assertEqual(len(content), 32768)
        self.assertEqual(len(self.read(content)["pad"]), 32758)
        for bad in (content + b' ', b''):
            with self.subTest(size=len(bad)), self.assertRaises(admission.PreflightError):
                self.read(bad)

    def test_bad_encoding_and_json_refuse(self):
        for bad in (b'\xff', b'{"x":'):
            with self.subTest(content=bad), self.assertRaises(admission.PreflightError):
                self.read(bad)

    def test_symlink_and_fifo_refuse_without_blocking(self):
        target = self.root / "target"
        target.write_text('{}')
        self.path.symlink_to(target)
        with self.assertRaises(admission.PreflightError):
            admission.read_report(self.path)
        self.path.unlink()
        os.mkfifo(self.path)
        with self.assertRaises(admission.PreflightError):
            admission.read_report(self.path)

    def test_path_replacement_cannot_bypass_the_checked_size(self):
        self.path.write_text('{"origin":"small"}')
        replacement = self.root / "large"
        replacement.write_text(json.dumps({"origin": "large", "padding": "x" * 40000}))
        real_lstat = Path.lstat
        def replace_after_check(path, *args, **kwargs):
            info = real_lstat(path, *args, **kwargs)
            if path == self.path and replacement.exists():
                replacement.replace(self.path)
            return info
        with mock.patch.object(Path, "lstat", replace_after_check):
            try:
                value = admission.read_report(self.path)
            except admission.PreflightError:
                return
        self.assertEqual(value["origin"], "small")

    def test_descriptor_stays_bound_when_path_changes_after_open(self):
        self.path.write_text('{"origin":"selected"}')
        replacement = self.root / "other"
        replacement.write_text('{"origin":"other"}')
        real_open = os.open
        def replace_after_open(path, *args, **kwargs):
            descriptor = real_open(path, *args, **kwargs)
            if Path(path) == self.path and replacement.exists():
                replacement.replace(self.path)
            return descriptor
        with mock.patch.object(os, "open", side_effect=replace_after_open):
            self.assertEqual(admission.read_report(self.path)["origin"], "selected")

    def test_report_fields_have_exact_declared_types(self):
        for key, value in [("observation_only", 1), ("criterion_pass", 0)]:
            value_report = report(self.root)
            value_report[key] = value
            with self.subTest(key=key), self.assertRaises(admission.PreflightError):
                admission.validate_report(value_report, self.root)
        for key in ("pid", "ppid"):
            for value in (42, True, "٤٢", "-1", " 1", ""):
                value_report = report(self.root)
                value_report["caller_identity"][key] = value
                with self.subTest(key=key, value=value), self.assertRaises(admission.PreflightError):
                    admission.validate_report(value_report, self.root)
        for value in (42, True, "b" * 39, "B" * 40, "z" * 40):
            value_report = report(self.root)
            value_report["caller_identity"]["code_cdhash"] = value
            with self.subTest(cdhash=value), self.assertRaises(admission.PreflightError):
                admission.validate_report(value_report, self.root)

    def test_stream_constructor_failure_closes_the_owned_descriptor(self):
        self.path.write_text('{}')
        real_open, descriptors = os.open, []
        def opened(*args, **kwargs):
            descriptor = real_open(*args, **kwargs)
            descriptors.append(descriptor)
            return descriptor
        with mock.patch.object(os, "open", side_effect=opened), \
                mock.patch.object(os, "fdopen", side_effect=OSError("owned constructor fixture")):
            with self.assertRaises(admission.PreflightError):
                admission.read_report(self.path)
        self.assertEqual(len(descriptors), 1)
        with self.assertRaises(OSError):
            os.fstat(descriptors[0])


class ManifestSelectionContracts(unittest.TestCase):
    def fixture(self):
        fixture = fixture_module.AdmissionContract("test_mocked_authenticated_observation_is_only_success_path")
        fixture.setUp()
        self.addCleanup(fixture.doCleanups)
        return fixture

    def test_changed_manifest_during_observation_refuses(self):
        original, other = self.fixture(), self.fixture()
        value_report = report(original.helper_app)
        def observe(command, **_kwargs):
            self.assertEqual(command[:3], ["/usr/bin/open", "-n", str(original.helper_app)])
            original.manifest.write_bytes(other.manifest.read_bytes())
            Path(command[command.index("--stdout") + 1]).write_text(json.dumps(value_report))
            return subprocess.CompletedProcess(command, 0)
        with mock.patch.object(admission.subprocess, "run", side_effect=observe):
            code, message = original.invoke()
        self.assertEqual(code, 1)
        self.assertIn("manifest changed", message)

    def test_changed_manifest_before_launch_refuses_and_removes_snapshot(self):
        original, other = self.fixture(), self.fixture()
        real_verified, snapshots = admission.verified_helper, []
        def mutate_after_capture(snapshot):
            snapshots.append(snapshot)
            original.manifest.write_bytes(other.manifest.read_bytes())
            selected = real_verified(snapshot)
            self.assertEqual(selected[1], original.helper_app)
            return selected
        with mock.patch.object(admission, "verified_helper", side_effect=mutate_after_capture), \
                mock.patch.object(admission.subprocess, "run") as launched:
            code, message = original.invoke()
        self.assertEqual(code, 1)
        self.assertIn("manifest changed", message)
        launched.assert_not_called()
        self.assertEqual(len(snapshots), 1)
        self.assertFalse(snapshots[0].exists())

    def test_unchanged_manifest_and_owned_observation_accept(self):
        original = self.fixture()
        before = original.manifest.read_bytes()
        def observe(command, **_kwargs):
            Path(command[command.index("--stdout") + 1]).write_text(json.dumps(report(original.helper_app)))
            return subprocess.CompletedProcess(command, 0)
        with mock.patch.object(admission.subprocess, "run", side_effect=observe):
            code, message = original.invoke()
        self.assertEqual(code, 0, message)
        self.assertEqual(original.manifest.read_bytes(), before)


if __name__ == "__main__":
    unittest.main(verbosity=2)
