"""Cross-layer cancellation contracts with owned files and virtual process clocks."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import struct
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/live-gates"))
import app_ui_host_diagnostic as host
import app_ui_host_process as process
import app_ui_host_cleanup as cleanup
import app_ui_host_manifest as manifest


class AppUIHostCleanupContracts(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="owned host archival ")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name).resolve()
        self.job = self.root / "running/job"
        self.job.mkdir(parents=True)
        self.private = self.job / "app-ui-private"
        self.private.mkdir(mode=0o700)
        self.bundle = self.private / "BridgeVMAppUIHost.app"
        binary = self.bundle / "Contents/MacOS/BridgeVMControl"
        binary.parent.mkdir(parents=True)
        cpu = 0x01000007 if manifest.platform.machine() == "x86_64" else 0x0100000C
        binary.write_bytes(struct.pack("<IIIIIIII", 0xFEEDFACF, cpu, 0, 2, 0, 0, 0, 0))
        self.digest = manifest.digest(binary)
        self.commit = "a" * 40
        self.input = self.job / "input-manifest.tsv"
        self.input.write_text(f"format\t{manifest.FORMAT}\ncommit\t{self.commit}\n"
                              f"binary\t{binary}\t{self.digest}\nlauncher\t{self.root / 'unlaunched'}\t{'b' * 64}\n")
        self.input_hash = manifest.digest(self.input)
        (self.job / "job.env").write_text(f"job_id=job\ntier={host.TIER}\ncommit={self.commit}\n"
                                         f"sealed_binary_sha256={self.digest}\ninput_manifest_sha256={self.input_hash}\n")
        self.receipt = dict(tier=host.TIER, commit=self.commit, job_id="job", binary_hash=self.digest,
                            input_manifest_sha256=self.input_hash, helper_sha256="b" * 64, run_count=1,
                            claim_eligible=False, criterion_pass=False, capability_promotion=False,
                            worker_cleanup_verified=False, **{"pass": False})
        self.launcher = dict(schema_version=1, kind="native-app-ui-launcher", host_pid=42, host_launch_date=100,
                             host_bundle_path=str(self.bundle), host_executable_path=str(binary),
                             host_executable_sha256=self.digest, identity_verified=True, exit_observed=True,
                             cleanup_verified=True, cancelled=True, timed_out=False, termination="term",
                             success=False, failure="owned cancellation")
        self.write_receipts()

    def write_receipts(self):
        (self.job / "receipt.json").write_text(json.dumps(self.receipt))
        if self.private.exists():
            (self.private / "launcher-observations.json").write_text(json.dumps(self.launcher))

    def test_outer_cleanup_allows_the_complete_late_owner_sequence(self):
        with tempfile.TemporaryDirectory(prefix="owned cleanup clock ") as directory:
            private = Path(directory)
            child = mock.Mock()
            child.poll.return_value = None
            # The launcher can acquire an owner just before5s, TERM for3s,
            # then observe exit during the following2s. No real clock advances.
            child.wait.return_value = 2
            with mock.patch.object(process.subprocess, "Popen", return_value=child) as launch, \
                    mock.patch.object(process.time, "monotonic", side_effect=[0, 91]):
                with self.assertRaises(TimeoutError):
                    host.run_launcher(private / "unlaunched", private / "unlaunched.app",
                                      private / "host-observations", "a" * 64,
                                      private / "launcher-observations.json", lambda: False)
            self.assertTrue((private / "cancel.requested").exists())
            self.assertNotIn("start_new_session", launch.call_args.kwargs)
            child.terminate.assert_called_once_with()
            child.wait.assert_called_once_with(timeout=12)
            child.kill.assert_not_called()

    def test_launcher_still_has_a_bounded_last_resort_when_it_never_acknowledges(self):
        child = mock.Mock()
        child.poll.return_value = None
        child.wait.side_effect = [subprocess.TimeoutExpired("fixture", 12), -9]
        with mock.patch.object(process.subprocess, "Popen", return_value=child), \
                mock.patch.object(process.time, "monotonic", side_effect=[0, 91]):
            with self.assertRaises(TimeoutError):
                process.run_launcher(self.root / "unlaunched", self.bundle, self.private / "host-observations",
                                     self.digest, self.private / "launcher-observations.json", lambda: False)
        self.assertTrue((self.private / "cancel.requested").is_file())
        child.terminate.assert_called_once_with()
        child.kill.assert_called_once_with()
        self.assertEqual(child.wait.call_args_list, [mock.call(timeout=12), mock.call(timeout=2)])

    def test_failed_ui_and_missing_completion_can_still_have_verified_owned_exit(self):
        # No host completion/UI report exists: the independent launcher owns exit.
        self.assertFalse((self.private / "host-observations").exists())
        cleanup.verify_job_cleanup(self.job, self.commit)
        self.assertFalse(json.loads((self.job / "receipt.json").read_text())["pass"])
        for field, value in (("host_pid", None), ("identity_verified", False), ("exit_observed", False),
                             ("cleanup_verified", False), ("host_executable_sha256", "c" * 64)):
            changed = dict(self.launcher, **{field: value})
            (self.private / "launcher-observations.json").write_text(json.dumps(changed))
            with self.assertRaises(ValueError):
                cleanup.verify_job_cleanup(self.job, self.commit)
        (self.private / "launcher-observations.json").unlink()
        with self.assertRaises(OSError):
            cleanup.verify_job_cleanup(self.job, self.commit)

    def test_preflight_refusal_requires_bound_receipt_and_no_private_host_directory(self):
        self.receipt["run_count"] = 0
        self.write_receipts()
        with self.assertRaises(ValueError):
            cleanup.verify_job_cleanup(self.job, self.commit)
        shutil.rmtree(self.private)
        cleanup.verify_job_cleanup(self.job, self.commit)
        for key, value in (("commit", "b" * 40), ("input_manifest_sha256", "b" * 64),
                           ("helper_sha256", "a" * 64), ("run_count", False)):
            changed = dict(self.receipt, **{key: value})
            (self.job / "receipt.json").write_text(json.dumps(changed))
            with self.assertRaises(ValueError):
                cleanup.verify_job_cleanup(self.job, self.commit)
        (self.job / "receipt.json").unlink()
        with self.assertRaises(OSError):
            cleanup.verify_job_cleanup(self.job, self.commit)

    def worker_wait(self, tier, residue=False):
        (self.job / "cancel.requested").touch()
        # Actual group wait/escalation functions use a virtual100ms clock. No
        # process is launched or signalled; only an owned shell evaluates policy.
        script = '''set -euo pipefail
source "$1/scripts/live-gates/live-process-cleanup.sh"
source "$1/scripts/live-gates/app-ui-host-worker-cleanup.sh"
ticks=0
mode="$5"
sleep() { ticks=$((ticks+1)); }
ps() { if [[ "${@: -1}" == "$$" ]]; then printf 88; else printf 42; fi; }
bridgevm_process_alive() { [[ "$mode" != residue ]] && (( ticks < 99 )); }
bridgevm_process_group_alive() { (( ticks < 99 )); }
wait() { return 1; }
trace="$2/signals"
kill() { printf '%s\\n' "$1" >> "$trace"; }
if bridgevm_wait_for_app_ui_host_group 42 "$2/cancel.requested" job "$3" "$2" "$1" "$4"; then
  printf 'allowed:%s:%s\\n' "$BRIDGEVM_TIER_STATUS" "$ticks"
  mv "$2" "$2.archived"
else
  printf 'fenced:%s:%s\\n' "$BRIDGEVM_TIER_STATUS" "$ticks"
fi
'''
        return subprocess.run(["bash", "-c", script, "owned-worker-clock", str(ROOT), str(self.job), tier, self.commit,
                               "residue" if residue else "cancel"],
                              capture_output=True, text=True, timeout=5)

    def test_new_tier_keeps_group_and_job_path_until_late_owned_cleanup(self):
        result = self.worker_wait(host.TIER)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("allowed:1:99", result.stdout)
        archived = Path(str(self.job) + ".archived")
        self.assertFalse(self.job.exists())
        self.assertEqual((archived / "signals").read_text().splitlines(), ["-TERM"])

    def test_old_tier_preserves_five_second_term_grace_without_host_cleanup_gate(self):
        (self.private / "launcher-observations.json").unlink()
        result = self.worker_wait("d6-app-ui")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("allowed:1:99", result.stdout)
        self.assertEqual((Path(str(self.job) + ".archived") / "signals").read_text().splitlines(), ["-TERM", "-KILL"])

    def test_residual_launcher_keeps_the_same_new_tier_cleanup_allowance(self):
        result = self.worker_wait(host.TIER, residue=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("allowed:1:99", result.stdout)
        self.assertEqual((Path(str(self.job) + ".archived") / "signals").read_text().splitlines(), ["-TERM"])

    def test_old_tier_residue_retains_the_original_five_second_term_allowance(self):
        result = self.worker_wait("d6-app-ui", residue=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("allowed:1:99", result.stdout)
        self.assertEqual((Path(str(self.job) + ".archived") / "signals").read_text().splitlines(), ["-TERM", "-KILL"])

    def test_unresolved_external_owner_fences_and_retains_running_job(self):
        self.launcher.update(exit_observed=False, cleanup_verified=False)
        self.write_receipts()
        result = self.worker_wait(host.TIER)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("fenced:126:99", result.stdout)
        self.assertTrue(self.job.exists())
        self.assertFalse(Path(str(self.job) + ".archived").exists())
        self.assertEqual((self.job / "signals").read_text().splitlines(), ["-TERM"])


if __name__ == "__main__":
    unittest.main(verbosity=2)
