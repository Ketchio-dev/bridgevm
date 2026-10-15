#!/usr/bin/env python3
"""Exercise both real preparation routes; never execute a fixture or application."""
import json
import shutil
import subprocess
import unittest
from unittest import mock

from app_ui_host_v2_fixtures import Fixture, ROOT, bundle, diagnostic, manifest
from app_ui_install_cancel_fixtures import Fixture as CancelFixture
import app_ui_host_v2_process as process
import app_ui_install_cancel_diagnostic as cancel_diagnostic


class SupervisorBundleContracts(unittest.TestCase):
    def executable(self, fixture):
        return fixture.private / "BridgeVMAppUIDriver.app/Contents/MacOS/AppUIHostLauncher"

    def prepare(self, fixture_type):
        fixture = fixture_type(self)
        shutil.rmtree(fixture.private)
        (fixture.output / "receipt.json").unlink()
        return fixture

    def route(self, module, fixture_type, flag):
        fixture = self.prepare(fixture_type)
        original = bundle.verify_bundles
        verified = []

        def verify(private, fields):
            original(private, fields, signatures=False)
            verified.append(private)

        def launch(*args, **kwargs):
            self.assertEqual(verified, [fixture.private])
            child = mock.Mock(returncode=0)
            child.poll.return_value = 0
            with mock.patch.object(process.subprocess, "Popen", return_value=child) as popen:
                result = process.run_launcher(*args, **kwargs)
            self.assertEqual(popen.call_count, 1)
            self.assertEqual(popen.call_args.args[0], [str(self.executable(fixture)), flag,
                "--private-root", str(fixture.private), "--host-sha256", fixture.fields["binary"][1],
                "--launcher-sha256", fixture.fields["launcher"][1]])
            self.assertEqual(self.executable(fixture).read_bytes(),
                (fixture.output / manifest.FILES["launcher"]).read_bytes())
            child.terminate.assert_not_called()
            child.kill.assert_not_called()
            return result

        # Only native signature evaluation is substituted during reconstruction.
        # The final result deliberately fails: command capture is not UI evidence.
        with mock.patch.object(bundle, "verify_bundles", side_effect=verify), \
                mock.patch.object(module, "run_launcher", side_effect=launch) as launched, \
                mock.patch.object(module, "verify_result", side_effect=ValueError("fixture has no UI result")):
            status = module.run(fixture.output, ROOT, fixture.commit,
                fixture.output / "input-manifest.tsv", fixture.output / manifest.FILES["binary"])
        self.assertEqual(status, 1)
        self.assertEqual(launched.call_count, 1)
        receipt = json.loads((fixture.output / "receipt.json").read_text())
        self.assertEqual(receipt["run_count"], 1)
        self.assertFalse(receipt["pass"])
        self.assertFalse(receipt["worker_cleanup_verified"])

    def test_overview_uses_verified_bundle_executable(self):
        self.route(diagnostic, Fixture, "--app-ui-supervisor-v2")

    def test_install_cancel_uses_verified_bundle_executable(self):
        self.route(cancel_diagnostic, CancelFixture, "--app-ui-install-cancel-supervisor-v1")

    def test_bare_outside_and_symlink_paths_are_refused_without_launch(self):
        for kind in ("bare", "outside", "alias", "bundle-symlink"):
            fixture = Fixture(self)
            bare = fixture.output / manifest.FILES["launcher"]
            outside = fixture.root / "same-bytes"
            outside.write_bytes(bare.read_bytes())
            alias = fixture.root / "driver-alias"
            alias.symlink_to(self.executable(fixture))
            path = {"bare": bare, "outside": outside, "alias": alias,
                    "bundle-symlink": self.executable(fixture)}[kind]
            if kind == "bundle-symlink":
                path.unlink()
                path.symlink_to(bare)
            with self.subTest(path=path), mock.patch.object(process.subprocess, "Popen") as popen:
                popen.return_value.poll.return_value = 0
                popen.return_value.returncode = 0
                with self.assertRaises(ValueError):
                    process.run_launcher(path, fixture.private, fixture.fields["binary"][1],
                        fixture.fields["launcher"][1], lambda: False)
                popen.assert_not_called()
                self.assertFalse((fixture.private / "launcher-v2.log").exists())

    def test_bundle_digest_mismatch_is_refused_without_launch(self):
        fixture = Fixture(self)
        with mock.patch.object(process.subprocess, "Popen") as popen:
            popen.return_value.poll.return_value = 0
            popen.return_value.returncode = 0
            with self.assertRaises(ValueError):
                process.run_launcher(self.executable(fixture), fixture.private,
                    fixture.fields["binary"][1], "0" * 64, lambda: False)
            popen.assert_not_called()
            self.assertFalse((fixture.private / "launcher-v2.log").exists())

    def test_signature_preparation_failure_never_reaches_either_launcher(self):
        for module, fixture_type in ((diagnostic, Fixture), (cancel_diagnostic, CancelFixture)):
            fixture = self.prepare(fixture_type)
            with self.subTest(tier=module.__name__), \
                    mock.patch.object(bundle, "verify_bundles", side_effect=subprocess.CalledProcessError(1, "codesign")), \
                    mock.patch.object(module, "run_launcher") as launched:
                status = module.run(fixture.output, ROOT, fixture.commit,
                    fixture.output / "input-manifest.tsv", fixture.output / manifest.FILES["binary"])
            self.assertEqual(status, 1)
            launched.assert_not_called()
            receipt = json.loads((fixture.output / "receipt.json").read_text())
            self.assertEqual(receipt["run_count"], 0)
            self.assertFalse(receipt["pass"])
            self.assertFalse(receipt["worker_cleanup_verified"])

    def test_launch_refusal_has_no_bare_retry(self):
        fixture = Fixture(self)
        with mock.patch.object(process.subprocess, "Popen", side_effect=OSError("launch refused")) as popen:
            with self.assertRaisesRegex(OSError, "launch refused"):
                process.run_launcher(self.executable(fixture), fixture.private,
                    fixture.fields["binary"][1], fixture.fields["launcher"][1], lambda: False)
        self.assertEqual(popen.call_count, 1)
        self.assertEqual(popen.call_args.args[0][0], str(self.executable(fixture)))

    def test_deadline_preserves_owned_child_cleanup_for_both_modes(self):
        for mode in ("--app-ui-supervisor-v2", "--app-ui-install-cancel-supervisor-v1"):
            fixture = Fixture(self)
            child = mock.Mock()
            child.poll.return_value = None
            child.wait.side_effect = [subprocess.TimeoutExpired("owned-supervisor", 12), 0]
            with self.subTest(mode=mode), mock.patch.object(process.subprocess, "Popen", return_value=child), \
                    mock.patch.object(process.time, "monotonic", side_effect=[0, process.DEADLINE_SECONDS]):
                with self.assertRaises(TimeoutError):
                    process.run_launcher(self.executable(fixture), fixture.private,
                        fixture.fields["binary"][1], fixture.fields["launcher"][1], lambda: False, mode=mode)
            child.terminate.assert_called_once_with()
            child.kill.assert_called_once_with()
            self.assertEqual(child.wait.call_args_list, [mock.call(timeout=12), mock.call(timeout=2)])
            self.assertTrue((fixture.private / "cancel.requested").is_file())


if __name__ == "__main__":
    unittest.main()
