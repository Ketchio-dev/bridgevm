#!/usr/bin/env python3
"""Exercise signing gates against real signed binaries and adversarial plists."""
import os
from pathlib import Path
import plistlib
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
PARSER = ROOT / 'scripts/verify-signing-entitlements.py'
HVF = 'com.apple.security.hypervisor'
DEBUG = 'com.apple.security.get-task-allow'
OTHER = 'com.apple.security.network.client'
AUDIO = 'com.apple.security.device.audio-input'


class EntitlementContracts(unittest.TestCase):
    def parse(self, raw, profile='release'):
        return subprocess.run(['python3', str(PARSER), HVF, '--profile', profile],
                              input=raw, capture_output=True).returncode

    def test_every_required_key_must_be_true(self):
        def parse(values, *keys):
            return subprocess.run(['python3', str(PARSER), *keys, '--profile', 'release'],
                                  input=plistlib.dumps(values), capture_output=True).returncode
        self.assertEqual(parse({HVF: True, AUDIO: True}, HVF, AUDIO), 0)
        self.assertNotEqual(parse({HVF: True}, HVF, AUDIO), 0)
        self.assertNotEqual(parse({HVF: True, AUDIO: 'true'}, HVF, AUDIO), 0)

    def test_exact_values_and_debug_policy(self):
        cases = [({HVF: True}, True), ({HVF: False, OTHER: True}, False),
                 ({HVF: 1}, False), ({HVF: 'true'}, False),
                 ({HVF: True, DEBUG: False, OTHER: True}, True),
                 ({HVF: True, DEBUG: True}, False),
                 ({HVF: True, DEBUG: 0}, False)]
        for values, accepted in cases:
            with self.subTest(values=values):
                self.assertEqual(self.parse(plistlib.dumps(values)) == 0, accepted)
        self.assertEqual(self.parse(plistlib.dumps({HVF: True, DEBUG: True}), 'debug'), 0)

    def test_malformed_duplicate_and_oversized(self):
        duplicate = f'<plist><dict><key>{HVF}</key><false/><key>{HVF}</key><true/></dict></plist>'.encode()
        for raw in (duplicate, b'<plist><dict>', b'', b'x' * 65537,
                    plistlib.dumps([True])):
            with self.subTest(raw=raw[:80]):
                self.assertNotEqual(self.parse(raw), 0)

    def test_signed_packaged_entitlements(self):
        """The runner needs the hypervisor; the probe also audio-input; the app audio-input."""
        with tempfile.TemporaryDirectory(prefix='bridgevm-signing-contract-') as temporary:
            root = Path(temporary)
            app = root / 'BridgeVM.app'
            (app / 'Contents/MacOS').mkdir(parents=True)
            (app / 'Contents/Info.plist').write_bytes(plistlib.dumps(
                {'CFBundleExecutable': 'BridgeVMControl', 'CFBundleIdentifier': 'dev.bridgevm.contract'}))
            subprocess.run(['cp', '/bin/echo', str(app / 'Contents/MacOS/BridgeVMControl')], check=True)
            runner = app / 'Contents/Resources/target/release/hvf-runner'
            probe = app / 'Contents/Resources/target/release/examples/hvf_gic_boot_probe'
            granted = {HVF: True, AUDIO: True}
            for runner_values, probe_values, app_values, accepted in [
                    ({HVF: True}, granted, {AUDIO: True}, True),
                    ({HVF: True, DEBUG: False, OTHER: True}, {**granted, DEBUG: False}, {AUDIO: True}, True),
                    ({HVF: True}, {HVF: True}, {AUDIO: True}, False),
                    ({HVF: True}, granted, {OTHER: True}, False),
                    ({OTHER: True}, granted, {AUDIO: True}, False),
                    ({HVF: False, OTHER: True}, granted, {AUDIO: True}, False),
                    ({HVF: True, DEBUG: True}, granted, {AUDIO: True}, False),
                    ({HVF: True}, {**granted, DEBUG: True}, {AUDIO: True}, False),
                    ({HVF: 1, OTHER: True}, granted, {AUDIO: True}, False),
                    ({HVF: True}, {HVF: True, AUDIO: 1}, {AUDIO: True}, False)]:
                with self.subTest(runner=runner_values, probe=probe_values, app=app_values):
                    # Nested code first: signing the bundle seals it.
                    for binary, values in [(runner, runner_values), (probe, probe_values), (app, app_values)]:
                        entitlements = root / 'entitlements.plist'
                        entitlements.write_bytes(plistlib.dumps(values))
                        if binary != app:
                            binary.parent.mkdir(parents=True, exist_ok=True)
                            subprocess.run(['cp', '/bin/echo', str(binary)], check=True)
                        subprocess.run(['codesign', '--force', '--sign', '-', '--entitlements',
                                        str(entitlements), str(binary)], check=True, capture_output=True)
                    result = subprocess.run([str(ROOT / 'scripts/verify-app-hvf-entitlements.sh'),
                                             str(app)], capture_output=True)
                    self.assertEqual(result.returncode == 0, accepted, result.stderr.decode())

    def test_codesign_extraction_failure_is_not_masked(self):
        with tempfile.TemporaryDirectory(prefix='bridgevm-signing-extraction-') as temporary:
            root = Path(temporary)
            fake = root / 'codesign'
            fake.write_text('#!/bin/sh\nif [ "$1" = --verify ]; then exit 0; fi\n'
                            'cat "$SIGNED_PLIST"\nexit 37\n')
            fake.chmod(0o755)
            plist = root / 'valid.plist'
            plist.write_bytes(plistlib.dumps({HVF: True}))
            app = root / 'BridgeVM.app'
            runner = app / 'Contents/Resources/target/release/hvf-runner'
            runner.parent.mkdir(parents=True)
            runner.write_text('fixture')
            runner.chmod(0o755)
            env = dict(os.environ, PATH=str(root) + ':' + os.environ['PATH'],
                       SIGNED_PLIST=str(plist))
            result = subprocess.run([str(ROOT / 'scripts/verify-app-hvf-entitlements.sh'),
                                     str(app)], env=env, capture_output=True)
            self.assertNotEqual(result.returncode, 0)

    def test_signed_builder_profiles(self):
        with tempfile.TemporaryDirectory(prefix='bridgevm-builder-signing-') as temporary:
            root = Path(temporary)
            # Only the release probe, under the hardened runtime, opens the microphone.
            for name, key, release_extra in [('hvf-runner', HVF, {}), ('hvf-windows-probe', HVF, {AUDIO: True}),
                                             ('apple-vz-runner', 'com.apple.security.virtualization', {})]:
                script = ROOT / f'apps/macos/scripts/build-sign-{name}.sh'
                cases = [({key: False, OTHER: True}, False, False),
                         ({key: True, DEBUG: True}, False, True),
                         ({key: True, DEBUG: True, **release_extra}, True, False),
                         ({key: True, DEBUG: False, OTHER: True, **release_extra}, True, True)]
                if release_extra:
                    cases.append(({key: True, DEBUG: False}, True, False))
                for values, release, accepted in cases:
                    with self.subTest(name=name, values=values, release=release):
                        binary = root / 'signed-echo'
                        subprocess.run(['cp', '/bin/echo', str(binary)], check=True)
                        entitlements = root / 'entitlements.plist'
                        entitlements.write_bytes(plistlib.dumps(values))
                        subprocess.run(['codesign', '--force', '--sign', '-', '--entitlements',
                                        str(entitlements), str(binary)], check=True, capture_output=True)
                        env = dict(os.environ, BRIDGEVM_HVF_RUNNER_SIGN_LOCK_DIR=str(root / 'lock'))
                        command = [str(script)] + (['--release'] if release else [])
                        result = subprocess.run(command + ['--verify-only', str(binary)],
                                                env=env, capture_output=True)
                        self.assertEqual(result.returncode == 0, accepted, result.stderr.decode())


if __name__ == '__main__':
    unittest.main()
