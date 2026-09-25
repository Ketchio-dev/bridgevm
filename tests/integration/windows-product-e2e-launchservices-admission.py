#!/usr/bin/env python3
"""Exercise T17 submission admission without changing system Accessibility trust."""
import contextlib
import importlib.util
import io
import hashlib, json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / "scripts/live-gates/windows-product-e2e-launchservices-preflight.py"
spec = importlib.util.spec_from_file_location("t17_admission", SOURCE)
assert spec and spec.loader
admission = importlib.util.module_from_spec(spec)
spec.loader.exec_module(admission)
manifest_module = admission.load_manifest_module()


class AdmissionContract(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="t17-admission-test.")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.app = self.root / "BridgeVMControl.app"
        self.helper_app = self.app / "Contents/Helpers/BridgeVMProductE2E.app"
        self.helper = self.helper_app / "Contents/MacOS/BridgeVMProductE2E"
        paths = {
            "app_executable": self.app / "Contents/MacOS/BridgeVMControl",
            "product_helper": self.helper,
            "runner": self.app / "Contents/Resources/target/release/hvf-runner",
            "firmware": self.app / "Contents/Resources/firmware/edk2-aarch64-secure-code.fd",
            "secure_boot_policy": self.app / "Contents/Resources/secureboot-microsoft-windows-transition-aarch64-v1.6.5.json",
            "iso": self.root / "Windows.iso",
            "bundled_vars_seed": self.app / "Contents/Resources/windows-boot-seed-vars.fd.gz",
            "guest_payload_manifest": self.root / "guest-payload.tsv",
        }
        for path in paths.values():
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(b"T17 admission fixture\n")
        paths["guest_payload"] = self.root / "guest-payload"
        paths["guest_payload"].mkdir()
        paths["app_bundle"] = self.app
        self.manifest = self.root / "manifest.tsv"
        lines = ["campaign_mode\tpilot"]
        for name in manifest_module.ASSETS:
            path = paths[name]
            digest = (manifest_module.tree_hash(path) if path.is_dir()
                      else manifest_module.file_hash(path))
            lines.append(f"{name}\t{path}\t{digest}")
        self.manifest.write_text("\n".join(lines) + "\n", encoding="utf-8")

    def invoke(self, manifest=None):
        output = io.StringIO()
        with mock.patch.object(sys, "platform", "darwin"), \
                mock.patch.object(sys, "argv", [str(SOURCE), "--manifest", str(manifest or self.manifest)]), \
                contextlib.redirect_stderr(output):
            result = admission.main()
        return result, output.getvalue()

    def test_missing_and_malformed_manifest_are_blocked(self):
        code, message = self.invoke(self.root / "missing.tsv")
        self.assertEqual(code, 1)
        self.assertIn("T17 manifest is invalid", message)
        self.manifest.write_text("bad\n", encoding="utf-8")
        code, message = self.invoke()
        self.assertEqual(code, 1)
        self.assertIn("T17 manifest is invalid", message)

    def test_missing_app_and_helper_are_blocked(self):
        self.helper.unlink()
        code, message = self.invoke()
        self.assertEqual(code, 1)
        self.assertIn("helper executable is missing", message)
        self.app.rename(self.root / "app-removed")
        code, message = self.invoke()
        self.assertEqual(code, 1)
        self.assertIn("app bundle is missing", message)

    def test_non_macos_is_blocked(self):
        with mock.patch.object(sys, "platform", "linux"):
            with self.assertRaisesRegex(admission.PreflightError, "requires macOS"):
                admission.preflight(self.manifest)

    def test_absent_launchservices_report_is_blocked(self):
        with mock.patch.object(admission.subprocess, "run", return_value=subprocess.CompletedProcess([], 0)), \
                mock.patch.object(admission.time, "monotonic", side_effect=[0, 11]):
            code, message = self.invoke()
        self.assertEqual(code, 1)
        self.assertIn("produced no report", message)

    def test_mocked_authenticated_observation_is_only_success_path(self):
        report = {
            "schema": "t17.accessibility-diagnostic.v1", "observation_only": True,
            "criterion_pass": False, "accessibility_trusted": True,
            "scope": "calling-process-only-not-product-e2e-or-tcc-database-attribution",
            "caller_identity": {
                "schema": "t17.caller-identity.v1", "pid": "42", "ppid": "1",
                "bundle_id": "dev.bridgevm.product-e2e", "bundle_path_sha256": hashlib.sha256(str(self.helper_app.resolve()).encode("utf-8")).hexdigest(),
                "executable_name": "BridgeVMProductE2E",
                "scope": "on-disk-code-metadata-not-signature-validation-or-tcc-attribution",
                "caller_status": "0", "static_code_status": "0", "signing_status": "0",
                "code_identifier": "dev.bridgevm.product-e2e", "code_cdhash": "b" * 40,
            },
        }

        def observe(command, **_kwargs):
            self.assertEqual(command[:2], ["/usr/bin/open", "-n"])
            self.assertEqual(command[2], str(self.helper_app))
            Path(command[command.index("--stdout") + 1]).write_text(json.dumps(report))
            return subprocess.CompletedProcess(command, 0)

        with mock.patch.object(admission.subprocess, "run", side_effect=observe) as launched:
            code, message = self.invoke()
        self.assertEqual(code, 0)
        self.assertIn("preflight passed", message)
        launched.assert_called_once()


if __name__ == "__main__":
    unittest.main(verbosity=2)
