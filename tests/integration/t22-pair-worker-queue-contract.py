#!/usr/bin/env python3
"""Actual worker refuses unproven T22 preparation without starting a guest."""
import hashlib
import json
import platform
import subprocess
import sys
import unittest

from t22_pair_worker_fixtures import WorkerFixture


@unittest.skipUnless(platform.system() == "Darwin", "copied worker uses fixed Darwin taskpolicy/caffeinate")
class WorkerQueue(unittest.TestCase):
    def setUp(self): self.fixture = WorkerFixture(self)

    def test_missing_private_receipt_fences_and_preserves_next_job(self):
        self.fixture.sealed_job(mode="missing")
        self.fixture.job(name="z-next")
        status, output = self.fixture.run()
        self.fixture.assert_fenced(status, output)

    def test_invalid_private_receipt_fences_and_preserves_next_job(self):
        self.fixture.sealed_job(mode="invalid")
        self.fixture.job(name="z-next")
        status, output = self.fixture.run()
        self.fixture.assert_fenced(status, output)

    def test_stale_missing_receipt_preserves_sealed_worktree(self):
        self.fixture.sealed_job(state="running")
        self.fixture.job(name="z-next")
        self.fixture.sealed_worktree()
        status, output = self.fixture.recover()
        self.fixture.assert_fenced(status, output)

    def test_stale_missing_sealed_worktree_never_falls_back_to_current_source(self):
        self.fixture.sealed_job(state="running", mode="invalid")
        self.fixture.job(name="z-next")
        status, output = self.fixture.recover()
        self.fixture.assert_fenced(status, output, worktree=False)
        self.assertFalse((self.fixture.work / "a-fixture").exists())

    def test_canceled_tier_without_measured_context_fences(self):
        self.fixture.sealed_job(mode="cancel")
        self.fixture.job(name="z-next")
        status, output = self.fixture.cancel()
        self.fixture.assert_fenced(status, output)

    def test_timed_out_tier_without_measured_context_fences(self):
        self.fixture.sealed_job(mode="timeout")
        self.fixture.job(name="z-next")
        status, output = self.fixture.run()
        self.fixture.assert_fenced(status, output)

    def refused_payload(self, mode):
        self.fixture.sealed_job(mode=mode)
        self.fixture.job(name="z-next")
        status, output = self.fixture.run()
        self.fixture.assert_fenced(status, output)
        stable = self.fixture.home / "BridgeVM/t22-prepared-pairs/a-fixture"
        self.assertTrue(stable.is_dir(), output)
        self.assertFalse((stable / "t22-input-manifest.tsv").exists(), output)
        return stable

    def test_cleanup_false_retains_owned_writable_media(self):
        stable = self.refused_payload("cleanup-false")
        self.assertTrue((stable / "live/vars.fd").stat().st_mode & 0o200)

    def test_unknown_spawn_retains_owned_writable_media(self):
        stable = self.refused_payload("unknown-spawn")
        self.assertTrue((stable / "live/vars.fd").stat().st_mode & 0o200)

    def test_standalone_cleanup_marker_fences_despite_clean_receipt_flags(self):
        stable = self.refused_payload("marker")
        self.assertTrue((stable / "cleanup-required.env").is_file())

    def test_missing_measured_launch_context_fences(self):
        self.refused_payload("missing-owned-context")

    def test_missing_query_completion_fences(self):
        self.refused_payload("missing-query-done")

    def test_checksum_matching_encrypted_data_query_fences(self):
        self.refused_payload("invalid-query")

    def test_unqueryable_launch_context_fences(self):
        self.refused_payload("unqueryable-group")

    def test_present_owned_group_fences_without_signaling_it(self):
        job = self.fixture.sealed_job(mode="live-group")
        pid = self.fixture.owned_group()
        owner = self.fixture.children[-1]
        (job / "fixture-group.pid").write_text(str(pid))
        self.fixture.job(name="z-next")
        status, output = self.fixture.run()
        self.fixture.assert_fenced(status, output)
        self.assertIsNone(owner.poll(), "guard signaled the retained owned fixture group")

    def finalized_payload(self, mode):
        self.fixture.sealed_job(mode=mode)
        status, output = self.fixture.run(debug=True)
        self.assertEqual(status, 0, output)
        self.assertFalse((self.fixture.queue / "worker-cleanup-required").exists(), output)
        done = self.fixture.queue / "done/a-fixture"
        self.assertTrue(done.is_dir(), output)
        self.assertFalse((self.fixture.queue / "running/a-fixture").exists(), output)
        self.assertFalse((self.fixture.work / "a-fixture").exists(), output)
        stable = self.fixture.home / "BridgeVM/t22-prepared-pairs/a-fixture"
        self.assertEqual(stable.stat().st_mode & 0o777, 0o500)
        for name in ("image.raw", "vars.fd"):
            self.assertEqual((stable / "live" / name).stat().st_mode & 0o777, 0o400)
        public = json.loads((done / "receipt.public.json").read_bytes())
        self.assertFalse(public["claim_eligible"])
        self.assertFalse(public["criterion_pass"])
        self.assertNotIn(str(self.fixture.root), json.dumps(public))
        return stable, done

    def test_successful_immutable_output_survives_done_move_and_worktree_removal(self):
        stable, done = self.finalized_payload("success")
        from native_snapshot_restore_inputs import parse_manifest, authenticate
        manifest = stable / "t22-input-manifest.tsv"; raw = manifest.read_bytes()
        rows = parse_manifest(raw, self.fixture.commit)
        authenticate(rows, __import__("pathlib").Path(rows["binary"][0]))
        self.assertEqual(rows["image"][0], str(stable / "live/image.raw"))
        self.assertEqual(rows["vars"][0], str(stable / "live/vars.fd"))
        private = json.loads((stable / "preparation-receipt.json").read_bytes())
        self.assertEqual(private["t22_input_manifest_sha256"], hashlib.sha256(raw).hexdigest())
        self.assertFalse(private["future_tpm_independence_proven"])

    def test_clean_failed_preparation_can_finalize_without_t22_input(self):
        stable, done = self.finalized_payload("clean-refusal")
        self.assertFalse((stable / "t22-input-manifest.tsv").exists())
        core = json.loads((stable / "preparation-receipt.json").read_bytes())
        self.assertFalse(core["preparation_complete"])
        self.assertFalse(core["natural_shutdown_observed"])
        self.assertEqual((done / "result.env").read_text(), "result=fail\nexit_code=1\n")

    def archived(self, done):
        return subprocess.run([sys.executable, str(self.fixture.repo / "scripts/live-gates/bridgevm_live_receipt.py"),
                               str(done), "a-fixture"], env=self.fixture.env(),
                              capture_output=True, text=True, timeout=10)

    def test_archived_cli_accepts_verified_done_after_worktree_removal(self):
        stable, done = self.finalized_payload("success")
        result = self.archived(done)
        self.assertEqual(result.returncode, 0, result.stderr)
        public = json.loads((done / "receipt.public.json").read_bytes())
        self.assertEqual(json.loads(result.stdout), public)
        self.assertFalse(public["pass"]); self.assertFalse(public["claim_eligible"])
        self.assertFalse(public["criterion_pass"])

    def test_archived_cli_refuses_changed_public_and_ledger_without_stdout(self):
        stable, done = self.finalized_payload("success")
        public = done / "receipt.public.json"; original = public.read_bytes()
        changed = json.loads(original); changed["preparation_receipt_sha256"] = "0" * 64
        public.chmod(0o600); public.write_text(json.dumps(changed)); public.chmod(0o400)
        result = self.archived(done)
        self.assertNotEqual(result.returncode, 0); self.assertEqual(result.stdout, "")
        public.chmod(0o600); public.write_bytes(original); public.chmod(0o400)
        ledger = self.fixture.queue / "job-ledger/a-fixture/entry.env"
        original = ledger.read_text()
        altered = "\n".join("origin_manifest_sha256=" + "0" * 64 if line.startswith("origin_manifest_sha256=") else line
                            for line in original.splitlines()) + "\n"
        ledger.chmod(0o600); ledger.write_text(altered); ledger.chmod(0o400)
        result = self.archived(done)
        self.assertNotEqual(result.returncode, 0); self.assertEqual(result.stdout, "")

    def test_stale_success_preserves_prepared_pair_after_done_move(self):
        directory = self.fixture.sealed_job(state="running", mode="success")
        worktree = self.fixture.sealed_worktree()
        self.fixture.emit(directory, worktree, "success")
        status, output = self.fixture.recover()
        self.assertEqual(status, 0, output)
        self.assertFalse((self.fixture.queue / "worker-cleanup-required").exists(), output)
        self.assertTrue((self.fixture.queue / "done/a-fixture/receipt.public.json").is_file())
        self.assertFalse(worktree.exists())
        stable = self.fixture.home / "BridgeVM/t22-prepared-pairs/a-fixture"
        self.assertTrue((stable / "t22-input-manifest.tsv").is_file())
        self.assertEqual(stable.stat().st_mode & 0o777, 0o500)

    def test_stale_cleanup_false_retains_running_and_blocks_next_job(self):
        directory = self.fixture.sealed_job(state="running", mode="cleanup-false")
        self.fixture.job(name="z-next")
        worktree = self.fixture.sealed_worktree()
        self.fixture.emit(directory, worktree, "cleanup-false")
        status, output = self.fixture.recover()
        self.fixture.assert_fenced(status, output)

    def test_stale_missing_query_completion_retains_running_and_blocks_next_job(self):
        directory = self.fixture.sealed_job(state="running", mode="missing-query-done")
        self.fixture.job(name="z-next")
        worktree = self.fixture.sealed_worktree()
        self.fixture.emit(directory, worktree, "missing-query-done")
        status, output = self.fixture.recover()
        self.fixture.assert_fenced(status, output)


if __name__ == "__main__": unittest.main()
