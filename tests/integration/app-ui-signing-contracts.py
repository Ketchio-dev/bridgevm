#!/usr/bin/env python3
"""Explicit diagnostic signer selection with no private-key or GUI operations."""
import contextlib
import importlib.util
import io
import os
from pathlib import Path
import platform
import plistlib
import struct
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("packager", ROOT / "apps/macos/scripts/package-app-ui-host-inputs.py")
packager = importlib.util.module_from_spec(spec)
spec.loader.exec_module(packager)
import app_ui_host_signing as signing

FINGERPRINT = "1234567890abcdef1234567890abcdef12345678"


class SigningContracts(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="bridgevm-signing-contract-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name).resolve()

    def arguments(self):
        return ["packager", "--host-binary", "/host", "--launcher-binary", "/driver",
                "--resource-commit", "a" * 40, "--output", "/output"]

    def test_cli_accepts_exact_fingerprint_and_normalizes_it(self):
        argv = self.arguments() + ["--signing-identity", FINGERPRINT]
        with patch.object(sys, "argv", argv), patch.object(packager, "package", return_value={"artifacts": {}}) as package:
            with contextlib.redirect_stdout(io.StringIO()):
                packager.main()
        package.assert_called_once_with("/host", "/driver", "a" * 40, "/output", FINGERPRINT.upper())

    def test_old_cli_ignores_signer_environment_and_keeps_default(self):
        with patch.object(sys, "argv", self.arguments()), patch.dict(os.environ, {"BRIDGEVM_CODESIGN_IDENTITY": FINGERPRINT}):
            with patch.object(packager, "package", return_value={"artifacts": {}}) as package:
                with contextlib.redirect_stdout(io.StringIO()):
                    packager.main()
        package.assert_called_once_with("/host", "/driver", "a" * 40, "/output", None)

    def test_cli_refuses_names_flags_whitespace_and_inexact_fingerprints(self):
        for value in ["-", "Apple Development: Fixture", "a" * 39, "g" * 40, FINGERPRINT + "\n", " " + FINGERPRINT]:
            with self.subTest(value=value), patch.object(sys, "argv", self.arguments() + ["--signing-identity", value]):
                with patch.object(packager, "package") as package, contextlib.redirect_stderr(io.StringIO()):
                    with self.assertRaises(SystemExit) as failure:
                        packager.main()
                self.assertEqual(failure.exception.code, 2)
                package.assert_not_called()

    def test_direct_invalid_selector_refuses_before_any_output_or_input_access(self):
        for value in ["-", "", "a" * 41, 123]:
            with self.subTest(value=value), patch.object(packager, "canonical") as canonical:
                with self.assertRaisesRegex(ValueError, "exact 40-hex"):
                    packager.package("/missing", "/missing", "a" * 40, self.root / "refused", value)
                canonical.assert_not_called()
        self.assertEqual(list(self.root.iterdir()), [])

    def bundle(self):
        bundle = self.root / "Fixture.app"
        executable = bundle / "Contents/MacOS/fixture"
        executable.parent.mkdir(parents=True)
        executable.write_bytes(b"mock code bytes, never executed")
        return bundle

    def test_default_command_is_unchanged_and_absolute_despite_environment(self):
        bundle = self.bundle()
        with patch.dict(os.environ, {"BRIDGEVM_CODESIGN_IDENTITY": FINGERPRINT, "PATH": str(self.root)}):
            with patch.object(signing.subprocess, "run", return_value=subprocess.CompletedProcess([], 0, "actual-output", "actual-error")) as run:
                record = signing.sign(bundle, "dev.bridgevm.app-ui-driver")
        commands = [call.args[0] for call in run.call_args_list]
        self.assertEqual(commands, [
            ["/usr/bin/codesign", "--force", "--sign", "-", "--identifier", "dev.bridgevm.app-ui-driver", "--timestamp=none", str(bundle)],
            ["/usr/bin/codesign", "--verify", "--strict", "--verbose=4", str(bundle)],
            ["/usr/bin/codesign", "--display", "--verbose=4", "--requirements", "-", str(bundle)]])
        for call in run.call_args_list:
            self.assertEqual(call.kwargs, {"capture_output": True, "text": True, "timeout": 20})
        self.assertEqual(record["display"]["stdout"], "actual-output")
        self.assertEqual(record["display"]["stderr"], "actual-error")
        self.assertEqual(set(record), {"sign", "verify", "display"})

    def test_explicit_signer_is_verified_by_leaf_requirement_and_recorded(self):
        bundle = self.bundle()
        metadata = 'Identifier=dev.bridgevm.app-ui-driver\ndesignated => identifier "dev.bridgevm.app-ui-driver"\n'
        with patch.object(signing.subprocess, "run", return_value=subprocess.CompletedProcess([], 0, "", metadata)) as run:
            record = signing.sign(bundle, "dev.bridgevm.app-ui-driver", FINGERPRINT)
        commands = [call.args[0] for call in run.call_args_list]
        self.assertEqual(commands[0][3], FINGERPRINT.upper())
        self.assertEqual(commands[1], ["/usr/bin/codesign", "--verify", "--strict", "--verbose=4", "-R",
                                      '=certificate leaf = H"' + FINGERPRINT.upper() + '"', str(bundle)])
        self.assertEqual(record["verify"]["arguments"], commands[1][1:])
        self.assertEqual(record["display"]["stderr"], metadata)
        self.assertNotIn("trusted", record)
        self.assertNotIn("notarized", record)

    def test_signing_timeout_propagates_without_retry_or_fallback(self):
        bundle = self.bundle()
        with patch.object(signing.subprocess, "run", side_effect=subprocess.TimeoutExpired("codesign", 20)) as run:
            with self.assertRaises(subprocess.TimeoutExpired):
                signing.sign(bundle, "dev.bridgevm.app-ui-driver", FINGERPRINT)
        self.assertEqual(run.call_count, 1)

    def inputs(self):
        cpu = 0x0100000C if platform.machine() == "arm64" else 0x01000007
        payload = struct.pack("<8I", 0xfeedfacf, cpu, 0, 2, 0, 0, 0, 0)
        paths = [self.root / "host", self.root / "driver"]
        for path in paths:
            path.write_bytes(payload)
        return paths, payload

    @staticmethod
    def git_resource(argv, **_kwargs):
        return b"7" if "-s" in argv else b"fixture"

    def test_sign_verify_or_display_failure_retains_partial_output_without_success_receipt(self):
        inputs, original = self.inputs()
        for failed_call in (1, 2, 3, 4):
            output = self.root / ("failed-" + str(failed_call))
            calls = []
            def codesign(argv, **kwargs):
                calls.append(argv)
                if len(calls) == failed_call:
                    return subprocess.CompletedProcess(argv, 1, "", "synthetic signature refusal")
                return subprocess.CompletedProcess(argv, 0, "", "")
            with self.subTest(failed_call=failed_call), patch.object(packager.subprocess, "check_output", side_effect=self.git_resource):
                with patch.object(signing.subprocess, "run", side_effect=codesign):
                    with self.assertRaisesRegex(ValueError, "synthetic signature refusal"):
                        packager.package(*inputs, "a" * 40, output, FINGERPRINT)
            self.assertEqual(len(calls), failed_call)
            self.assertTrue((output / "BridgeVMAppUIHost.app/Contents/MacOS/BridgeVMControl").exists())
            self.assertFalse((output / "packaging-inputs.json").exists())
            self.assertFalse((output / "AppUIHostLauncher").exists())
            for path in inputs:
                self.assertEqual(path.read_bytes(), original)

    def test_mock_certificate_package_seals_signed_driver_copy_and_preserves_inputs(self):
        inputs, original = self.inputs()
        output = self.root / "signed"
        def codesign(argv, **kwargs):
            bundle = Path(argv[-1])
            if "--force" in argv:
                executable = next((bundle / "Contents/MacOS").iterdir())
                executable.write_bytes(executable.read_bytes() + b"synthetic-signature")
                resources = bundle / "Contents/_CodeSignature/CodeResources"
                resources.parent.mkdir()
                resources.write_bytes(b"synthetic-resource-seal")
            return subprocess.CompletedProcess(argv, 0, "mock actual display", "")
        with patch.object(packager.subprocess, "check_output", side_effect=self.git_resource):
            with patch.object(signing.subprocess, "run", side_effect=codesign):
                record = packager.package(*inputs, "a" * 40, output, FINGERPRINT)
        driver = output / "BridgeVMAppUIDriver.app/Contents/MacOS/AppUIHostLauncher"
        self.assertNotEqual(driver.read_bytes(), original)
        self.assertEqual(driver.read_bytes(), (output / "AppUIHostLauncher").read_bytes())
        self.assertEqual(record["artifacts"]["launcher"]["sha256"], packager.digest(driver.read_bytes()))
        self.assertTrue(record["same_driver_supervisor_bytes"])
        self.assertTrue(record["original_inputs_unchanged"])
        self.assertFalse(record["gui_launched"])
        self.assertFalse(record["accessibility_trust_observed"])
        for path in inputs:
            self.assertEqual(path.read_bytes(), original)
        for role in ("host", "driver"):
            self.assertEqual(record["signatures"][role]["sign"]["arguments"][2], FINGERPRINT.upper())
            self.assertEqual(record["signatures"][role]["display"]["stdout"], "mock actual display")


@unittest.skipUnless(sys.platform == "darwin", "actual requirement check needs macOS codesign")
class CertificateRequirementContract(unittest.TestCase):
    def test_valid_exact_leaf_requirement_rejects_a_real_ad_hoc_fixture(self):
        with tempfile.TemporaryDirectory(prefix="bridgevm-certificate-refusal-") as temporary:
            root = Path(temporary).resolve()
            bundle = root / "Fixture.app"
            executable = bundle / "Contents/MacOS/fixture"
            executable.parent.mkdir(parents=True)
            source = root / "fixture.c"
            source.write_text("int main(void) { return 0; }\n")
            subprocess.run(["xcrun", "clang", "-target", platform.machine() + "-apple-macos14.0",
                            str(source), "-o", str(executable)], check=True, timeout=30, capture_output=True)
            (bundle / "Contents/Info.plist").write_bytes(plistlib.dumps({
                "CFBundleExecutable": "fixture", "CFBundlePackageType": "APPL",
                "CFBundleIdentifier": "dev.bridgevm.app-ui-driver"}))
            requirement = '=certificate leaf = H"' + FINGERPRINT.upper() + '"'
            parsed = subprocess.run(["/usr/bin/csreq", "-r", requirement, "-t"],
                                    timeout=10, capture_output=True, text=True)
            self.assertEqual(parsed.returncode, 0, parsed.stderr)
            record = signing.sign(bundle, "dev.bridgevm.app-ui-driver")
            self.assertEqual(record["verify"]["exit_code"], 0)
            with self.assertRaisesRegex(ValueError, "failed to satisfy specified code requirement"):
                signing.code(["--verify", "--strict", "--verbose=4", "-R", requirement, str(bundle)])


if __name__ == "__main__":
    unittest.main()
