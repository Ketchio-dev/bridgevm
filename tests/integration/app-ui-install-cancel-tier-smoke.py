#!/usr/bin/env python3
"""Synthetic queue/evidence rejection contracts; never execute a GUI or grant AX."""
import copy
import json
import os
import subprocess
import unittest
from unittest import mock

from app_ui_install_cancel_fixtures import Fixture, ROOT, save, digest, manifest, cleanup, protocol, observations
import app_ui_host_v2_manifest as old_manifest
import app_ui_host_v2_cleanup as old_cleanup
import app_ui_host_v2_process as process
import app_ui_install_cancel_result as result
from app_ui_install_cancel_rejection_cases import EvidenceRejections


class InstallCancelContracts(unittest.TestCase):
    def setUp(self):
        self.f = Fixture(self)

    def test_versioned_manifest_is_not_interchangeable_with_v2(self):
        f = self.f
        manifest.validate_manifest(f.input, f.commit)
        with self.assertRaises(ValueError):
            old_manifest.validate_manifest(f.input, f.commit)
        f.input.write_text(f.text.replace(manifest.FORMAT, old_manifest.FORMAT))
        old_manifest.validate_manifest(f.input, f.commit)
        with self.assertRaises(ValueError):
            manifest.validate_manifest(f.input, f.commit)

    def test_actual_submit_seals_six_files_in_fresh_synthetic_queue_only(self):
        f = self.f
        queue = f.root / "synthetic queue"
        result = subprocess.run([str(ROOT / "scripts/live-gates/bridgevm-live"), "submit", manifest.TIER,
            "--sha", f.commit, "--input-manifest", str(f.input), "--job-id", "install-cancel-fixture"],
            env=dict(os.environ, BRIDGEVM_LIVE_ROOT=str(queue)), capture_output=True, text=True, timeout=10)
        self.assertEqual(result.returncode, 0, result.stderr)
        job = queue / "queued/install-cancel-fixture"
        self.assertFalse((job / "app-ui-private").exists())
        for key, (_, expected) in f.fields.items():
            self.assertEqual(digest(job / manifest.FILES[key]), expected)

    def test_exact_fixed_transcript_and_unchanged_fixture_are_accepted(self):
        f = self.f
        cleanup.verify_job_cleanup(f.output, f.commit)
        completion_hash, reads = protocol.verify_driver(f.private, f.launch)
        self.assertEqual(completion_hash, digest(f.private / "install-cancel-completion.json"))
        self.assertEqual(reads, f.reads)
        self.assertEqual(observations.verify_observations(f.observed, reads),
                         digest(f.observed / "install-cancel-observations.json"))

    def test_cleanup_cannot_accept_old_kind_or_missing_driver_exit(self):
        f = self.f
        for mutate in (lambda value: value.update(kind="native-app-ui-launcher-v2"),
                       lambda value: value["driver"].update(exit_observed=False)):
            value = copy.deepcopy(f.launch)
            mutate(value)
            save(f.private / manifest.LAUNCHER_FILE, value)
            with self.assertRaises(ValueError):
                cleanup.verify_job_cleanup(f.output, f.commit)
        with self.assertRaises(ValueError):
            old_cleanup.verify_job_cleanup(f.output, f.commit)

    def test_driver_success_needs_exact_counts_trust_and_session(self):
        f = self.f
        for key, value in (("mutationsPerformed", 6), ("mutationsPerformed", True),
                           ("requestsProcessed", 7), ("requestsProcessed", 1025),
                           ("hostPID", 4243), ("sessionSHA256", "b" * 64), ("success", False)):
            save(f.private / "install-cancel-completion.json", dict(f.completion, **{key: value}))
            with self.subTest(key=key), self.assertRaises(ValueError):
                protocol.verify_driver(f.private, f.launch)
        save(f.private / "install-cancel-completion.json", f.completion)
        save(f.private / "install-cancel-ready.json", dict(f.ready, trusted=False))
        with self.assertRaises(ValueError):
            protocol.verify_driver(f.private, f.launch)

    def test_request_reply_binding_and_no_mutation_replay(self):
        f = self.f
        path = f.private / "driver-replies/000004.json"
        original = json.loads(path.read_text())
        for key, value in (("requestSHA256", "b" * 64), ("outcome", "refused"),
                           ("hostPID", 4243), ("sequence", True), ("kind", "native-app-ui-driver-reply")):
            save(path, dict(original, **{key: value}))
            with self.subTest(key=key), self.assertRaises(ValueError):
                protocol.verify_driver(f.private, f.launch)
        save(path, original)
        (f.private / "driver-requests/000009.json").write_bytes((f.private / "driver-requests/000002.json").read_bytes())
        with self.assertRaises(ValueError):
            protocol.verify_driver(f.private, f.launch)

    def test_changed_phase_deadline_cannot_extend_a_poll(self):
        f = self.f
        request_path = f.private / "driver-requests/000003.json"
        reply_path = f.private / "driver-replies/000003.json"
        request, reply = json.loads(request_path.read_text()), json.loads(reply_path.read_text())
        request["phaseDeadlineUptime"] += 1
        save(request_path, request)
        reply["requestSHA256"] = digest(request_path)
        save(reply_path, reply)
        with self.assertRaises(ValueError):
            protocol.verify_driver(f.private, f.launch)

    def test_actual_readback_and_checkpoint_reference_are_required(self):
        f = self.f
        path = f.private / "driver-replies/000005.json"
        value = json.loads(path.read_text())
        value["values"]["cancelEnabled"] = True
        save(path, value)
        with self.assertRaises(ValueError):
            protocol.verify_driver(f.private, f.launch)
        f.report["checkpoints"][2]["read_sequence"] = 4
        f.write_report()
        with self.assertRaises(ValueError):
            observations.verify_observations(f.observed, f.reads)

    def test_all_forbidden_effects_and_gate_timeout_refuse(self):
        f = self.f
        for key, count in observations.COUNTERS.items():
            f.report["counters"][key] = count + 1
            f.write_report()
            with self.subTest(key=key), self.assertRaises(ValueError):
                observations.verify_observations(f.observed, f.reads)
            f.report["counters"][key] = count

    def test_changed_or_additional_fixture_files_are_not_cleanup_success(self):
        f = self.f
        path = f.observed / observations.FIXTURE_FILES[0]
        original = path.read_bytes()
        path.write_bytes(b"changed registration")
        with self.assertRaises(ValueError):
            observations.verify_observations(f.observed, f.reads)
        path.write_bytes(original)
        (f.observed / "fixture-library/pending/bundle/disk.raw").write_bytes(b"unexpected media")
        with self.assertRaises(ValueError):
            observations.verify_observations(f.observed, f.reads)

    def test_new_launcher_flag_preserves_finite_cancel_cleanup(self):
        f = self.f
        child = mock.Mock()
        child.poll.return_value = None
        child.wait.side_effect = [subprocess.TimeoutExpired("owned-supervisor", 12), 0]
        with mock.patch.object(process.subprocess, "Popen", return_value=child) as popen:
            with self.assertRaises(TimeoutError):
                process.run_launcher(f.output / manifest.FILES["launcher"], f.private,
                    f.fields["binary"][1], f.fields["launcher"][1], lambda: True,
                    mode="--app-ui-install-cancel-supervisor-v1", log_name="install-cancel-launcher.log")
        self.assertEqual(popen.call_args.args[0][1], "--app-ui-install-cancel-supervisor-v1")
        self.assertEqual(child.wait.call_args_list, [mock.call(timeout=12), mock.call(timeout=2)])
        self.assertEqual(child.terminate.call_count, 1)
        self.assertEqual(child.kill.call_count, 1)
        self.assertTrue((f.private / "cancel.requested").is_file())

    def test_complete_result_requires_host_settlement_and_exact_report_binding(self):
        f = self.f
        # Only the actual signature operation is replaced for fake Mach-O fixtures.
        verify = result.verify_bundles
        with mock.patch.object(result, "verify_bundles", side_effect=lambda p, v: verify(p, v, signatures=False)):
            self.assertEqual(result.verify_result(f.private, f.fields, 0),
                (digest(f.observed / "install-cancel-observations.json"), digest(f.private / "install-cancel-completion.json")))
            for key, value in (("settled", False), ("fixturePreserved", False), ("pid", 4243),
                               ("reportSHA256", "b" * 64), ("kind", "native-app-ui-host-completion")):
                save(f.observed / "install-cancel-host-completion.json", dict(f.host_completion, **{key: value}))
                with self.subTest(key=key), self.assertRaises(ValueError):
                    result.verify_result(f.private, f.fields, 0)
            save(f.observed / "install-cancel-host-completion.json", f.host_completion)
            with self.assertRaises(ValueError):
                result.verify_result(f.private, f.fields, 1)

    def test_worker_cleanup_dispatch_requires_both_owned_exits(self):
        f = self.f
        f.launch["driver"]["exit_observed"] = False
        save(f.private / manifest.LAUNCHER_FILE, f.launch)
        script = '''source "$1/scripts/live-gates/app-ui-host-worker-cleanup.sh"
bridgevm_wait_for_tier_group() { return 0; }
BRIDGEVM_TIER_STATUS=0
bridgevm_wait_for_app_ui_host_group 123 "$2/cancel.requested" fixture d6-app-ui-install-cancel-v1 "$2" "$1" "$3"
test "$?" = 1 && test "$BRIDGEVM_TIER_STATUS" = 126
'''
        executed = subprocess.run(["bash", "-c", script, "fixture", str(ROOT), str(f.output), f.commit],
                                 capture_output=True, text=True, timeout=10)
        self.assertEqual(executed.returncode, 0, executed.stderr)


if __name__ == "__main__":
    unittest.main()
