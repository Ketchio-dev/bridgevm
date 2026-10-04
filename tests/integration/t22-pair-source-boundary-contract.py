#!/usr/bin/env python3
"""Dirty sealed code never executes to authenticate its own D10 cleanup."""
import os
from pathlib import Path
import subprocess
import unittest

from t22_pair_worker_fixtures import WorkerFixture, TIER
from t22_pair_shell_environment import run_in_trusted_shell


class SourceBoundary(unittest.TestCase):
    def setUp(self):
        self.fixture = WorkerFixture(self)
        self.directory = self.fixture.sealed_job(state="running", mode="success")
        self.fixture.job(name="z-next")
        self.worktree = self.fixture.sealed_worktree()
        self.marker = self.fixture.root / "dirty-code-executed"

    def dirty(self, name):
        path = self.worktree / "scripts/live-gates" / name
        prefix = f"from pathlib import Path\nPath({str(self.marker)!r}).write_text('dirty')\nraise SystemExit(0)\n"
        path.write_text(prefix + path.read_text())

    def guard(self, env=None):
        command = ['source "$1"; bridgevm_t17_guard_or_fence "$2" "$3" "$4" "$5" a-fixture "$6"',
                   "_", str(self.fixture.repo / "scripts/live-gates/t17-worker-cleanup-fence.sh"),
                   TIER, str(self.directory), str(self.worktree), self.fixture.commit, str(self.fixture.queue)]
        result = run_in_trusted_shell(command, env or self.fixture.env())
        self.fixture.assert_fenced(result.returncode, result.stdout + result.stderr)
        self.assertFalse(self.marker.exists(), "dirty verifier/import ran before source admission")

    def test_exact_head_dirty_verifier_cannot_accept_invalid_receipt(self):
        (self.directory / "receipt.json").write_text('{"unverified":true}\n')
        self.dirty("t22_pair_queue.py")
        self.guard()

    def test_exact_head_dirty_import_cannot_accept_invalid_receipt(self):
        (self.directory / "receipt.json").write_text('{"unverified":true}\n')
        self.dirty("t22_pair_queue_proof.py")
        self.guard()

    def test_stale_valid_proofs_still_refuse_dirty_import_before_done(self):
        self.fixture.emit(self.directory, self.worktree, "success")
        self.dirty("t22_pair_queue_proof.py")
        status, output = self.fixture.recover()
        self.fixture.assert_fenced(status, output)
        self.assertFalse(self.marker.exists())
        self.assertTrue((self.fixture.home / "BridgeVM/t22-prepared-pairs/a-fixture").is_dir())

    def test_supported_publication_and_run_dispatch_refuse_dirty_verifier(self):
        (self.directory / "receipt.json").write_text('{"unverified":true}\n')
        self.dirty("t22_pair_queue.py")
        for mode in ("finalize", "publish", "guard", "run"):
            with self.subTest(mode=mode):
                argv = [str(self.directory), str(self.worktree), self.fixture.commit]
                if mode == "guard": argv.append("a-fixture")
                if mode == "run": argv.extend((str(self.directory / "input-manifest.tsv"), str(self.directory / "hvf_gic_boot_probe")))
                result = subprocess.run(["/bin/bash", "--noprofile", "--norc", "-p",
                    str(self.worktree / "scripts/live-gates/t22-pair-queue-dispatch.sh"), mode, *argv],
                    env=self.fixture.env(), capture_output=True, text=True, timeout=35)
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse(self.marker.exists())

    def test_controlled_admission_ignores_foreign_git_and_shell_loader_environment(self):
        admission = self.fixture.repo / "scripts/live-gates/t22-pair-source-admission.sh"
        startup = self.fixture.root / "startup.sh"
        startup.write_text(f"touch '{self.marker}'\nexit 0\n")
        env = dict(self.fixture.env(), GIT_DIR="/never", GIT_WORK_TREE="/never", BASH_ENV=str(startup),
                   ENV=str(startup), DYLD_INSERT_LIBRARIES="/never", LD_PRELOAD="/never")
        body = 'source "$1" "$2" "$3"; [[ -z ${DYLD_INSERT_LIBRARIES+x} && -z ${LD_PRELOAD+x} ]]'
        command = [body, "_", str(admission), str(self.worktree), self.fixture.commit]
        result = run_in_trusted_shell(command, env)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse(self.marker.exists())
        self.dirty("t22_pair_queue.py")
        self.guard(env)


if __name__ == "__main__": unittest.main()
