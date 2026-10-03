#!/usr/bin/env python3
"""Execute the installer launcher using only owned synthetic files and a fake probe.

The fixture replaces the firmware pin in its copied policy with its synthetic
3 MiB payload hash. Production has no fixture pin or executable override.
Host signing/process inventory and guest verification are fixture commands;
no Hypervisor.framework, app/UI, mount, or real guest media is involved.
"""
from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import shutil
import re
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
POLICY_NAME = "run-hvf-windows-scripted-install-policy.sh"
PIN = "b1dc201b1382476ca8c8dcbf8c09abc7ae7429c8437e35bffd54bb9b228b750b"
PAYLOAD = b"owned-firmware-fixture".ljust(3 * 1024 * 1024, b"\0")


class ScriptedInstallFirmwareContract(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="bridgevm-installer-firmware.", dir="/tmp")
        self.addCleanup(self.temporary.cleanup)
        self.base = Path(self.temporary.name)
        self.runtime = self.base / "Moved App With Spaces.app/Contents/Resources"
        self.scripts = self.runtime / "scripts"
        self.scripts.mkdir(parents=True)
        self.runner = self.scripts / "run-hvf-windows-scripted-install.sh"
        shutil.copyfile(ROOT / "scripts" / self.runner.name, self.runner)
        policy = ROOT / "scripts" / POLICY_NAME
        if policy.is_file():
            text = policy.read_text()
            self.assertEqual(text.count(PIN), 1)
            (self.scripts / POLICY_NAME).write_text(
                text.replace(PIN, hashlib.sha256(PAYLOAD).hexdigest()))
        self.firmware = self.runtime / "firmware/edk2-aarch64-secure-code.fd"
        self.firmware.parent.mkdir()
        self.firmware.write_bytes(PAYLOAD)
        self.source = self.base / "source.fixture"
        self.target = self.base / "target.fixture"
        self.vars = self.base / "vars.fixture"
        for path in (self.source, self.target, self.vars):
            path.write_bytes(b"owned-synthetic-placeholder")
        self.evidence = self.base / "evidence"
        self.capture = self.base / "probe-firmware.txt"
        self.verified = self.base / "target-verified.txt"
        self.shims = self.base / "host-fixtures"
        self.shims.mkdir()
        self.executable(self.shims / "codesign", "printf '<key>com.apple.security.hypervisor</key><true/>\\n'\n")
        self.executable(self.shims / "stat", "printf 'fixture-stat\\n'\n")
        self.executable(self.shims / "pgrep", "exit 1\n")
        self.executable(self.shims / "tmux", "exit 1\n")
        self.executable(self.scripts / "verify-hvf-windows-install-target.sh",
                        'printf "verified\\n" > "$FIXTURE_VERIFIED"\n')
        for profile in ("debug", "release"):
            probe = self.runtime / "target" / profile / "examples/hvf_gic_boot_probe"
            probe.parent.mkdir(parents=True)
            self.executable(probe,
                'printf "%s\\n" "${BRIDGEVM_AARCH64_UEFI_CODE-<absent>}" > "$FIXTURE_CAPTURE"\n')

    @staticmethod
    def executable(path, body):
        path.write_text("#!/bin/bash\nset -eu\n" + body)
        path.chmod(0o700)

    def invoke(self, extra_environment=None, extra_arguments=(), release=True):
        environment = {k: v for k, v in os.environ.items() if not k.startswith("BRIDGEVM_")}
        environment.update(PATH=f"{self.shims}:/usr/bin:/bin:/usr/sbin:/sbin",
                           FIXTURE_CAPTURE=str(self.capture), FIXTURE_VERIFIED=str(self.verified))
        environment.update(extra_environment or {})
        arguments = ["/bin/bash", str(self.runner), "--source", str(self.source),
                     "--target", str(self.target), "--vars", str(self.vars),
                     "--evidence-dir", str(self.evidence), "--skip-build",
                     "--watchdog-ms", "1500000"]
        if release:
            arguments.append("--release")
        result = subprocess.run(arguments + list(extra_arguments), env=environment,
                                capture_output=True, text=True, timeout=10)
        return result

    def assert_admitted(self, result, expected):
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertTrue(self.capture.is_file(), "fake probe did not execute")
        self.assertEqual(self.capture.read_text().strip(), str(expected))
        self.assertTrue(self.verified.is_file(), "target verifier did not execute")
        preflight = (self.evidence / "preflight.txt").read_text()
        self.assertIn(f"BRIDGEVM_AARCH64_UEFI_CODE={expected}", preflight)
        self.assertIn("firmware_sha256=" + hashlib.sha256(PAYLOAD).hexdigest(), preflight)

    def assert_refused(self, result, reason="firmware"):
        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn(reason, result.stderr.lower())
        self.assertFalse(self.capture.exists(), "invalid firmware reached the probe")
        self.assertFalse(self.evidence.exists(), "admission refusal created evidence")
        for path in (self.source, self.target, self.vars):
            self.assertEqual(path.read_bytes(), b"owned-synthetic-placeholder")

    def test_packaged_firmware_survives_relocation(self):
        self.assert_admitted(self.invoke(), self.firmware)

    def test_inherited_code_and_repository_overrides_do_not_change_release_selection(self):
        foreign = self.base / "foreign-code.fd"
        foreign.write_bytes(PAYLOAD)
        self.assert_admitted(self.invoke({"BRIDGEVM_AARCH64_UEFI_CODE": str(foreign),
                                         "BRIDGEVM_REPO_ROOT": str(self.base)}), self.firmware)

    def test_repository_development_selects_its_own_pinned_layout(self):
        development = self.base / "owned repository"
        shutil.move(str(self.runtime), development)
        self.runtime = development
        self.scripts = development / "scripts"
        self.runner = self.scripts / self.runner.name
        firmware = development / "crates/bridgevm-hvf/firmware/edk2-aarch64-secure-code.fd"
        firmware.parent.mkdir(parents=True)
        shutil.move(str(development / "firmware/edk2-aarch64-secure-code.fd"), firmware)
        self.assert_admitted(self.invoke(release=False), firmware)

    def test_missing_bundle_code_cannot_fall_back_to_repository_layout(self):
        fallback = self.runtime / "crates/bridgevm-hvf/firmware/edk2-aarch64-secure-code.fd"
        fallback.parent.mkdir(parents=True)
        fallback.write_bytes(PAYLOAD)
        self.firmware.unlink()
        self.assert_refused(self.invoke())

    def test_truncated_firmware_is_refused(self):
        self.firmware.write_bytes(b"small")
        self.assert_refused(self.invoke())

    def test_same_size_corrupted_firmware_is_refused(self):
        self.firmware.write_bytes(b"!" + PAYLOAD[1:])
        self.assert_refused(self.invoke())

    def test_symlinked_firmware_is_refused(self):
        foreign = self.base / "foreign-code.fd"
        foreign.write_bytes(PAYLOAD)
        self.firmware.unlink()
        self.firmware.symlink_to(foreign)
        self.assert_refused(self.invoke())

    def test_refusal_precedes_destructive_media_options(self):
        template = self.base / "template.fixture"
        template.write_bytes(b"owned-template")
        self.firmware.unlink()
        self.assert_refused(self.invoke(extra_arguments=("--fresh-target-size", "4m",
                                                        "--vars-template", str(template))))

    def test_policy_resource_cannot_be_missing_or_a_symlink(self):
        policy = self.scripts / POLICY_NAME
        owned_copy = self.base / "policy-copy.sh"
        shutil.move(policy, owned_copy)
        self.assert_refused(self.invoke(), reason="policy resource")
        policy.symlink_to(owned_copy)
        self.assert_refused(self.invoke(), reason="policy resource")

    def test_package_producer_copies_the_policy_from_its_exact_source_path(self):
        source = (ROOT / "apps/macos/scripts/package-hvf-control-app.sh").read_text()
        loop = re.search(r"for script in \\\n.*?\ndone", source, re.S)
        self.assertIsNotNone(loop)
        stage_app = self.base / "owned-copy.app"
        (stage_app / "Contents/Resources/scripts").mkdir(parents=True)
        result = subprocess.run(["/bin/bash", "-eu", "-c", loop.group()],
                                env={"ROOT": str(ROOT), "stage_app": str(stage_app),
                                     "PATH": "/usr/bin:/bin:/usr/sbin:/sbin"},
                                capture_output=True, text=True, timeout=10)
        self.assertEqual(result.returncode, 0, result.stderr)
        copied = stage_app / "Contents/Resources/scripts" / POLICY_NAME
        self.assertEqual(copied.read_bytes(), (ROOT / "scripts" / POLICY_NAME).read_bytes())
        self.assertTrue(os.access(copied, os.X_OK))
        resources = (ROOT / "apps/macos/Sources/BridgeVMControl/HvfEngine/HvfWindowsInstallResources.swift").read_text()
        self.assertIn('"scripts/' + POLICY_NAME + '"', resources)

    def test_production_pin_matches_the_product_policy_and_bundle_producer(self):
        policy_path = ROOT / "apps/macos/Sources/BridgeVMControl/Resources/secureboot-microsoft-windows-transition-aarch64-v1.6.5.json"
        self.assertEqual(json.loads(policy_path.read_text())["firmware"]["sha256"], PIN)
        self.assertIn('SECURE_FIRMWARE_SHA256="' + PIN + '"',
                      (ROOT / "apps/macos/scripts/package-hvf-control-app.sh").read_text())
        self.assertIn('expected_sha="' + PIN + '"', (ROOT / "scripts" / POLICY_NAME).read_text())


if __name__ == "__main__":
    unittest.main()
