#!/usr/bin/env python3
"""V2 queue and evidence faults; never launch either diagnostic application."""
import copy
import json
import os
import shutil
import subprocess
import unittest
from unittest import mock

from app_ui_host_v2_fixtures import Fixture, ROOT, manifest, bundle, cleanup, diagnostic, digest
import app_ui_host_v2_process as process


class V2Contracts(unittest.TestCase):
    def setUp(self):
        self.f = Fixture(self)

    def test_actual_submit_seals_all_six_inputs_before_publication(self):
        f = self.f
        env = dict(os.environ, BRIDGEVM_LIVE_ROOT=str(f.root / "queue"))
        command = [str(ROOT / "scripts/live-gates/bridgevm-live"), "submit", manifest.TIER,
                   "--sha", f.commit, "--input-manifest", str(f.input), "--job-id", "dual-host"]
        result = subprocess.run(command, env=env, capture_output=True, text=True, timeout=10)
        self.assertEqual(result.returncode, 0, result.stderr)
        job = f.root / "queue/queued/dual-host"
        for key, (_, expected) in f.fields.items():
            self.assertEqual(digest(job / manifest.FILES[key]), expected)
        f.fields["driver_info"][0].write_bytes(b"changed source")
        self.assertEqual(digest(job / manifest.FILES["driver_info"]), f.fields["driver_info"][1])
        self.assertFalse((job / "app-ui-private").exists())
        self.assertNotEqual(subprocess.run(command, env=env, capture_output=True, timeout=10).returncode, 0)

    def test_manifest_refuses_missing_duplicate_unknown_v1_and_bad_hash(self):
        f = self.f
        variants = [f.text.replace(manifest.FORMAT, "bridgevm-app-ui-host-v1"),
            f.text + "driver_info\tignored\tignored\n", f.text + "command\t/bin/true\n",
            "\n".join(f.text.splitlines()[:-1]) + "\n", f.text.replace(f.fields["driver_info"][1], "0" * 64)]
        for text in variants:
            f.input.write_text(text)
            with self.subTest(text=text[:70]), self.assertRaises(ValueError):
                manifest.validate_manifest(f.input, f.commit)

    def test_worker_claim_resolves_symlink_queue_root_to_physical_job_path(self):
        f = self.f
        queue = f.root / "physical queue"
        env = dict(os.environ, BRIDGEVM_LIVE_ROOT=str(queue))
        cli = str(ROOT / "scripts/live-gates/bridgevm-live")
        submitted = subprocess.run([cli, "submit", manifest.TIER, "--sha", f.commit,
            "--input-manifest", str(f.input), "--job-id", "alias-claim"],
            env=env, capture_output=True, text=True, timeout=10)
        self.assertEqual(submitted.returncode, 0, submitted.stderr)
        alias = f.root / "queue alias"
        alias.symlink_to(queue, target_is_directory=True)
        env["BRIDGEVM_LIVE_ROOT"] = str(alias)
        claimed = subprocess.run([cli, "next"], env=env, capture_output=True, text=True, timeout=10)
        self.assertEqual(claimed.returncode, 0, claimed.stderr)
        self.assertEqual(claimed.stdout.strip(), str(queue / "running/alias-claim"))
        self.assertEqual(manifest.parse_manifest(queue / "running/alias-claim/input-manifest.tsv", f.commit), f.fields)

    def test_metadata_symlink_ancestor_and_changed_copy_are_refused(self):
        f = self.f
        alias = f.root / "alias"
        alias.symlink_to(f.sources, target_is_directory=True)
        f.input.write_text(f.text.replace(str(f.fields["driver_info"][0]), str(alias / manifest.FILES["driver_info"])))
        with self.assertRaises(ValueError):
            manifest.validate_manifest(f.input, f.commit)
        f.input.write_text(f.text)
        f.fields["driver_resources"][0].write_bytes(b"changed")
        with self.assertRaises(ValueError):
            manifest.validate_manifest(f.input, f.commit)

    def test_bundle_hash_metadata_and_required_signature_verifier(self):
        f = self.f
        bundle.verify_bundles(f.private, f.fields, signatures=False)
        with mock.patch.object(bundle.sys, "platform", "darwin"), mock.patch.object(bundle.subprocess, "run") as run:
            bundle.verify_bundles(f.private, f.fields)
        self.assertEqual(run.call_count, 2)
        self.assertTrue(all(c.args[0][:4] == ["/usr/bin/codesign", "--verify", "--deep", "--strict"] for c in run.call_args_list))
        (f.private / "BridgeVMAppUIDriver.app/Contents/Info.plist").write_bytes(b"changed")
        with self.assertRaises(ValueError):
            bundle.verify_bundles(f.private, f.fields, signatures=False)

    def test_reconstruction_uses_sealed_copies_and_exact_git_resources(self):
        f = self.f
        destination = f.root / "reconstructed"
        destination.mkdir(mode=0o700)
        original = bundle.verify_bundles
        with mock.patch.object(bundle, "verify_bundles", side_effect=lambda p, v: original(p, v, signatures=False)):
            bundle.reconstruct_bundles(destination, f.output, ROOT, f.commit, f.fields)
        for name in bundle.RESOURCES:
            expected = subprocess.check_output(["git", "-C", str(ROOT), "show",
                f"{f.commit}:apps/macos/Sources/BridgeVMControl/Resources/{name}"])
            self.assertEqual((destination / "BridgeVMAppUIHost.app/Contents/Resources/BridgeVMApp_BridgeVMControl.bundle" / name).read_bytes(), expected)
        self.assertEqual((destination / "BridgeVMAppUIDriver.app/Contents/MacOS/AppUIHostLauncher").read_bytes(),
                         (f.output / manifest.FILES["launcher"]).read_bytes())
        with self.assertRaises(FileExistsError):
            bundle.reconstruct_bundles(destination, f.output, ROOT, f.commit, f.fields)

    def test_code_signature_failure_aborts_reconstruction_before_launch(self):
        f = self.f
        with mock.patch.object(bundle.sys, "platform", "darwin"), mock.patch.object(bundle.subprocess, "run",
                side_effect=subprocess.CalledProcessError(1, "codesign")):
            with self.assertRaises(subprocess.CalledProcessError):
                bundle.verify_bundles(f.private, f.fields)

    def test_both_independent_exits_are_required_and_v1_receipt_is_refused(self):
        f = self.f
        cleanup.verify_job_cleanup(f.output, f.commit)

        for key, value in [("exit_observed", False), ("identity_verified", False), ("cleanup_verified", False),
                           ("pid", 4242), ("pid", True), ("executable_sha256", "0" * 64)]:
            original = f.roles["driver"][key]
            f.roles["driver"][key] = value
            f.save()
            with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                cleanup.verify_job_cleanup(f.output, f.commit)
            f.roles["driver"][key] = original
        f.launch["schema_version"] = 1
        f.save()
        with self.assertRaises(ValueError):
            cleanup.verify_job_cleanup(f.output, f.commit)

    def test_driver_requirement_metadata_matches_swift_limits(self):
        f = self.f
        ready, _ = f.write_driver()
        for value in ["x" * 129, "fixture\0reason"]:
            changed = dict(ready, codeRequirementUnavailableReason=value)
            (f.private / "driver-ready.json").write_text(json.dumps(changed))
            with self.assertRaises(ValueError):
                diagnostic.verify_driver(f.private, f.launch)

    def test_proven_no_launch_cleans_but_unadmitted_request_never_does(self):
        f = self.f
        role = f.roles["driver"]
        role.update(pid=None, launch_date=None, identity_verified=False, exit_observed=False,
                    launch_requested=False, no_launch_verified=True, failure="prelaunch-refused")
        f.launch.update(success=False, failure="prelaunch-refused")
        f.save()
        cleanup.verify_job_cleanup(f.output, f.commit)
        role["launch_requested"] = True
        f.save()
        with self.assertRaises(ValueError):
            cleanup.verify_job_cleanup(f.output, f.commit)

    def test_missing_driver_duplicate_keys_wrong_deadline_and_flag_conflicts(self):
        f = self.f
        original = copy.deepcopy(f.launch)
        for mutate in [lambda x: x.pop("driver"), lambda x: x.update(deadline_uptime=195),
                       lambda x: x.update(cancelled=True), lambda x: x.update(cleanup_verified=False)]:
            f.launch = copy.deepcopy(original)
            mutate(f.launch)
            f.save()
            with self.assertRaises(ValueError):
                cleanup.verify_job_cleanup(f.output, f.commit)
        path = f.private / "launcher-observations-v2.json"
        path.write_text(json.dumps(original)[:-1] + ',"success":true}')
        with self.assertRaises(ValueError):
            cleanup.verify_job_cleanup(f.output, f.commit)

    def test_driver_success_requires_trust_exact_binding_and_complete_action_counts(self):
        f = self.f
        ready, completion = f.write_driver()
        diagnostic.verify_driver(f.private, f.launch)
        for key, value in [("driverPID", 9999), ("requestsProcessed", 11), ("mutationsPerformed", 5),
                           ("mutationsPerformed", True), ("success", False), ("sessionSHA256", "b" * 64)]:
            changed = dict(completion, **{key: value})
            (f.private / "driver-completion.json").write_text(json.dumps(changed))
            with self.subTest(key=key), self.assertRaises(ValueError):
                diagnostic.verify_driver(f.private, f.launch)
        (f.private / "driver-completion.json").write_text(json.dumps(completion))
        ready.update(trusted=False, failureCode="accessibility-untrusted")
        (f.private / "driver-ready.json").write_text(json.dumps(ready))
        with self.assertRaises(ValueError):
            diagnostic.verify_driver(f.private, f.launch)
        cleanup.verify_job_cleanup(f.output, f.commit)

    def test_worker_fences_a_host_exit_without_driver_exit(self):
        f = self.f
        f.roles["driver"]["exit_observed"] = False
        f.save()
        script = '''source "$1/scripts/live-gates/app-ui-host-worker-cleanup.sh"
bridgevm_wait_for_tier_group() { return 0; }
BRIDGEVM_TIER_STATUS=0
bridgevm_wait_for_app_ui_host_group 123 "$2/cancel.requested" fixture d6-app-ui-host-v2 "$2" "$1" "$3"
test "$?" = 1 && test "$BRIDGEVM_TIER_STATUS" = 126
'''
        result = subprocess.run(["bash", "-c", script, "fixture", str(ROOT), str(f.output), f.commit],
                                capture_output=True, text=True, timeout=10)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_supervisor_cancel_stays_bounded_and_never_signals_an_app_by_pid(self):
        f = self.f
        child = mock.Mock()
        child.poll.return_value = None
        child.wait.side_effect = [subprocess.TimeoutExpired("owned-supervisor", 12), 0]
        with mock.patch.object(process.subprocess, "Popen", return_value=child) as popen:
            with self.assertRaises(TimeoutError):
                process.run_launcher(f.output / manifest.FILES["launcher"], f.private,
                    f.fields["binary"][1], f.fields["launcher"][1], lambda: True)
        self.assertEqual(popen.call_args.args[0], [str(f.output / manifest.FILES["launcher"]),
            "--app-ui-supervisor-v2", "--private-root", str(f.private), "--host-sha256",
            f.fields["binary"][1], "--launcher-sha256", f.fields["launcher"][1]])
        self.assertEqual(child.terminate.call_count, 1)
        self.assertEqual(child.kill.call_count, 1)
        self.assertEqual(child.wait.call_args_list, [mock.call(timeout=12), mock.call(timeout=2)])
        self.assertTrue((f.private / "cancel.requested").is_file())

    def test_prelaunch_zero_run_is_not_accepted_when_private_state_exists(self):
        f = self.f
        f.receipt["run_count"] = 0
        f.save()
        with self.assertRaises(ValueError):
            cleanup.verify_job_cleanup(f.output, f.commit)
        shutil.rmtree(f.private)
        cleanup.verify_job_cleanup(f.output, f.commit)


if __name__ == "__main__":
    unittest.main()
