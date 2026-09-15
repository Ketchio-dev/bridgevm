#!/usr/bin/env python3
"""Owned manifest/receipt contracts only: no native app or queue job executes."""
import copy
import json
import os
from pathlib import Path
import struct
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/live-gates"))
import app_ui_diagnostic as original
import app_ui_host_diagnostic as host
import app_ui_host_manifest as manifest
from app_ui_host_cleanup_cases import AppUIHostCleanupContracts
from app_ui_host_bundle_fixtures import png

COMMIT = subprocess.check_output(["git", "-C", str(ROOT), "rev-parse", "HEAD"], text=True).strip()
CLI = ROOT / "scripts/live-gates/bridgevm-live"


class AppUIHostContracts(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="bridgevm owned host contract ")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name).resolve()
        self.environment = dict(os.environ, BRIDGEVM_LIVE_ROOT=str(self.root / "queue"))
        self.binary, self.launcher = self.root / "owned host", self.root / "owned launcher"
        cpu = 0x01000007 if manifest.platform.machine() == "x86_64" else 0x0100000C
        header = struct.pack("<IIIIIIII", 0xFEEDFACF, cpu, 0, 2, 0, 0, 0, 0)
        self.binary.write_bytes(header + b"owned host data, never executable")
        self.launcher.write_bytes(header + b"owned launcher data, never executable")
        self.binary_hash, self.launcher_hash = map(original.digest, (self.binary, self.launcher))
        self.input = self.root / "inputs.tsv"
        self.text = (f"format\t{manifest.FORMAT}\ncommit\t{COMMIT}\n"
                     f"binary\t{self.binary}\t{self.binary_hash}\n"
                     f"launcher\t{self.launcher}\t{self.launcher_hash}\n")
        self.input.write_text(self.text)

    def submit(self, name="owned-host"):
        return subprocess.run([str(CLI), "submit", host.TIER, "--sha", COMMIT,
                               "--input-manifest", str(self.input), "--job-id", name],
                              env=self.environment, text=True, capture_output=True, timeout=10)

    def job(self, name="owned-host"):
        result = self.submit(name)
        self.assertEqual(result.returncode, 0, result.stderr)
        return self.root / "queue/queued" / name

    def write_reports(self, observations, bundle, launcher_report):
        screenshots = []
        for name in original.SCREENSHOTS:
            path = observations / (name + ".png")
            path.write_bytes(png())
            screenshots.append(dict(name=name, file=path.name, sha256=original.digest(path), width=1, height=1))
        report = dict(schema_version=1, kind="native-app-ui-diagnostic", fixture_data=True,
                      guest_behavior_proof=False, failure=None, screenshots=screenshots,
                      actions=dict.fromkeys(original.ACTIONS, True), tripwires=dict.fromkeys(original.TRIPWIRES, 0))
        (observations / "ui-observations.json").write_text(json.dumps(report))
        identity = dict(schema_version=1, kind="native-app-ui-host-identity", pid=4242,
                        bundle_identifier=host.BUNDLE_ID, bundle_path=str(bundle),
                        executable_path=str(bundle / "Contents/MacOS/BridgeVMControl"),
                        executable_sha256=self.binary_hash, started_uptime=123.5)
        completion = dict(schema_version=1, kind="native-app-ui-host-completion", pid=4242, success=True,
                          cleanup_verified=True, report_sha256=original.digest(observations / "ui-observations.json"),
                          failure=None)
        launcher = dict(schema_version=1, kind="native-app-ui-launcher", host_pid=4242,
                        host_launch_date=1700000000.5, host_bundle_path=identity["bundle_path"],
                        host_executable_path=identity["executable_path"], host_executable_sha256=self.binary_hash,
                        identity_verified=True, exit_observed=True, cleanup_verified=True, cancelled=False,
                        timed_out=False, termination="none", success=True, failure=None)
        paths = (observations / "host-identity.json", observations / "host-completion.json", launcher_report)
        for path, value in zip(paths, (identity, completion, launcher)):
            path.write_text(json.dumps(value))
        return paths, (identity, completion, launcher)

    def prepared(self):
        private = self.root / "app-ui-private"
        private.mkdir(mode=0o700)
        bundle = host.reconstruct_bundle(private, ROOT, COMMIT, self.binary, self.binary_hash)
        observations = private / "host-observations"
        observations.mkdir(mode=0o700)
        return bundle, observations, private / "launcher-observations.json"

    def test_actual_cli_seals_both_executables_before_publication(self):
        job = self.job()
        self.assertEqual((job / "input-manifest.tsv").read_bytes(), self.input.read_bytes())
        self.assertEqual((job / "hvf_gic_boot_probe").read_bytes(), self.binary.read_bytes())
        self.assertEqual((job / manifest.LAUNCHER).read_bytes(), self.launcher.read_bytes())
        self.binary.write_bytes(b"changed source")
        self.launcher.write_bytes(b"changed launcher")
        manifest.verify_executable(job / "hvf_gic_boot_probe", self.binary_hash)
        manifest.verify_executable(job / manifest.LAUNCHER, self.launcher_hash)
        self.assertEqual(host.load_job(job, COMMIT, job / "input-manifest.tsv", self.binary_hash), "owned-host")
        self.assertFalse((job / "app-ui-private").exists())
        self.assertFalse((job / "run.log").exists())
        self.assertNotEqual(self.submit().returncode, 0)

    def test_actual_cli_refuses_invalid_manifest_and_executable_types(self):
        variants = [self.text.replace(manifest.FORMAT, original.FORMAT),
                    self.text.replace(COMMIT, "0" * 40), self.text + "command\t/bin/true\n",
                    self.text + "launcher\textra\textra\n", self.text.replace(str(self.launcher), "relative"),
                    self.text.replace(self.launcher_hash, "0" * 64),
                    "\n".join(self.text.splitlines()[:-1]) + "\n"]
        for value in variants:
            self.input.write_text(value)
            self.assertNotEqual(self.submit().returncode, 0)
            self.assertFalse((self.root / "queue").exists())
        old = self.launcher.read_bytes()
        self.launcher.write_bytes(old[:12] + struct.pack("<I", 8) + old[16:])
        self.input.write_text(self.text.replace(self.launcher_hash, original.digest(self.launcher)))
        self.assertNotEqual(self.submit().returncode, 0, "MH_BUNDLE must not be accepted as MH_EXECUTE")
        self.launcher.write_bytes(old)
        alias = self.root / "alias"
        alias.symlink_to(self.launcher)
        self.input.write_text(self.text.replace(str(self.launcher), str(alias)))
        self.assertNotEqual(self.submit().returncode, 0)
        self.assertFalse((self.root / "queue").exists())

    def test_companion_seal_rechecks_source_and_refuses_existing_destination(self):
        stage = self.root / "stage"
        stage.mkdir()
        (stage / "hvf_gic_boot_probe").write_bytes(self.binary.read_bytes())
        old = self.launcher.read_bytes()
        self.launcher.write_bytes(old + b"changed since validation")
        with self.assertRaises(ValueError):
            manifest.seal_launcher(self.input, COMMIT, stage)
        self.assertFalse((stage / manifest.LAUNCHER).exists())
        self.launcher.write_bytes(old)
        manifest.seal_launcher(self.input, COMMIT, stage)
        with self.assertRaises(FileExistsError):
            manifest.seal_launcher(self.input, COMMIT, stage)
        self.assertEqual((stage / manifest.LAUNCHER).read_bytes(), old)

    def test_real_dispatch_refuses_each_changed_copy_without_launch(self):
        for index, name in enumerate(("hvf_gic_boot_probe", manifest.LAUNCHER)):
            job = self.job(f"changed-{index}")
            path = job / name
            path.chmod(0o700)
            path.write_bytes(b"changed sealed file")
            result = subprocess.run([str(ROOT / "scripts/live-gates/run-tier.sh"), host.TIER,
                                     "--out", str(job), "--job-id", f"changed-{index}",
                                     "--input-manifest", str(job / "input-manifest.tsv"),
                                     "--sealed-binary", str(job / "hvf_gic_boot_probe")],
                                    text=True, capture_output=True, timeout=10)
            self.assertNotEqual(result.returncode, 0)
            receipt = json.loads((job / "receipt.json").read_text())
            self.assertEqual(receipt["run_count"], 0)
            self.assertEqual(receipt["boots_attempted"], 0)
            for field in ("pass", "claim_eligible", "criterion_pass", "capability_promotion", "worker_cleanup_verified"):
                self.assertIs(receipt[field], False)
            self.assertFalse((job / "app-ui-private").exists())
            subprocess.run([str(ROOT / "scripts/live-gates/publish-receipt.sh"), host.TIER, str(job), str(ROOT), COMMIT],
                           check=True, capture_output=True, timeout=10)
            public = (job / "receipt.public.json").read_text()
            self.assertNotIn(str(self.root), public)
            self.assertEqual(json.loads(public)["helper_sha256"], self.launcher_hash)

    def test_job_binds_tier_commit_manifest_and_primary_seal(self):
        job = self.job()
        path = job / "job.env"
        baseline = path.read_text()
        for old, new in ((host.TIER, original.TIER), (COMMIT, "0" * 40),
                         (self.binary_hash, "f" * 64), (original.digest(self.input), "e" * 64)):
            path.write_text(baseline.replace(old, new))
            with self.assertRaises(ValueError):
                host.load_job(job, COMMIT, job / "input-manifest.tsv", self.binary_hash)
        path.write_text(baseline + "commit=" + COMMIT + "\n")
        with self.assertRaises(ValueError):
            host.load_job(job, COMMIT, job / "input-manifest.tsv", self.binary_hash)

    def test_bundle_reconstruction_uses_exact_git_resources_and_unmodified_binary(self):
        bundle, _, _ = self.prepared()
        self.assertEqual((bundle / "Contents/MacOS/BridgeVMControl").read_bytes(), self.binary.read_bytes())
        info = host.plistlib.loads((bundle / "Contents/Info.plist").read_bytes())
        self.assertEqual(info["LSMinimumSystemVersion"], "14.0")
        resources = bundle / "Contents/Resources/BridgeVMApp_BridgeVMControl.bundle"
        self.assertEqual({path.name for path in resources.iterdir()}, set(host.RESOURCES))
        for name in host.RESOURCES:
            expected = subprocess.check_output(["git", "-C", str(ROOT), "cat-file", "blob",
                f"{COMMIT}:apps/macos/Sources/BridgeVMControl/Resources/{name}"], timeout=5)
            self.assertEqual((resources / name).read_bytes(), expected)

    def test_complete_identity_agreement_still_requires_original_observations(self):
        bundle, observations, launch = self.prepared()
        self.write_reports(observations, bundle, launch)
        expected = original.digest(observations / "ui-observations.json")
        self.assertEqual(host.verify_result(observations, bundle, self.binary_hash, launch, 0), expected)
        path = observations / "ui-observations.json"
        report = json.loads(path.read_text())
        report["actions"]["search_cleared"] = False
        path.write_text(json.dumps(report))
        with self.assertRaises(ValueError):
            host.verify_result(observations, bundle, self.binary_hash, launch, 0)

    def test_lifecycle_refuses_missing_mismatched_cancelled_or_incomplete_ownership(self):
        bundle, observations, launch = self.prepared()
        paths, values = self.write_reports(observations, bundle, launch)
        changes = [(("pid", True), ("bundle_path", "/other"), ("executable_sha256", "0" * 64),
                    ("started_uptime", float("nan"))),
                   (("pid", 4243), ("success", False), ("cleanup_verified", False), ("report_sha256", "f" * 64)),
                   (("host_pid", 4243), ("host_launch_date", None), ("host_executable_path", "/other"),
                    ("identity_verified", False), ("exit_observed", False), ("cleanup_verified", False),
                    ("cancelled", True), ("timed_out", True), ("termination", "kill"), ("success", False))]
        for path, baseline, variants in zip(paths, values, changes):
            for key, value in variants:
                changed = copy.deepcopy(baseline)
                changed[key] = value
                path.write_text(json.dumps(changed))
                with self.assertRaises(ValueError):
                    host.verify_result(observations, bundle, self.binary_hash, launch, 0)
            path.unlink()
            with self.assertRaises(OSError):
                host.verify_result(observations, bundle, self.binary_hash, launch, 0)
            path.write_text(json.dumps(baseline))
        with self.assertRaises(ValueError):
            host.verify_result(observations, bundle, self.binary_hash, launch, 1)

    def test_runner_exit_alone_refuses_and_complete_fake_boundary_remains_nonpromoting(self):
        for complete in (False, True, "clean-failure"):
            job = self.job(f"composition-{complete}")
            def fake_launcher(_launcher, bundle, observations, _expected, report, _canceled):
                if complete:
                    self.write_reports(observations, bundle, report)
                if complete == "clean-failure":
                    (observations / "ui-observations.json").unlink()
                return 0
            with mock.patch.object(host.sys, "platform", "darwin"), \
                    mock.patch.object(host, "run_launcher", side_effect=fake_launcher) as child:
                result = host.run(job, ROOT, COMMIT, job / "input-manifest.tsv", job / "hvf_gic_boot_probe")
            self.assertEqual(result, 0 if complete is True else 1)
            self.assertEqual(child.call_count, 1)
            receipt = json.loads((job / "receipt.json").read_text())
            self.assertIs(receipt["pass"], complete is True)
            self.assertIs(receipt["worker_cleanup_verified"], bool(complete))
            self.assertEqual(receipt["boots_attempted"], 0)
            for field in ("claim_eligible", "criterion_pass", "capability_promotion"):
                self.assertIs(receipt[field], False)

    def test_prelaunch_cancel_skips_launcher_and_deadline_cleanup_is_bounded(self):
        job = self.job()
        (job / "cancel.requested").touch()
        with mock.patch.object(host.sys, "platform", "darwin"), mock.patch.object(host, "run_launcher") as child:
            self.assertEqual(host.run(job, ROOT, COMMIT, job / "input-manifest.tsv", job / "hvf_gic_boot_probe"), 1)
        child.assert_not_called()
        self.assertEqual(json.loads((job / "receipt.json").read_text())["run_count"], 0)


if __name__ == "__main__":
    unittest.main(verbosity=2)
