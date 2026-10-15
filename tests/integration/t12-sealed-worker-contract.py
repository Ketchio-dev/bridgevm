#!/usr/bin/env python3
"""Exercise actual worker preflight; stop before worktree or tier execution."""
import subprocess
import unittest
from t12_sealed_runtime_fixture import Runtime

HARNESS = r'''
source "$1"
git() {
    if [[ "$3" == worktree && "$4" == add ]]; then
        printf stopped > "$T12_WORKTREE_BOUNDARY"
        return 1
    fi
    command git "$@"
}
run_job "$2"
'''


class WorkerContract(unittest.TestCase):
    def setUp(self):
        self.f = Runtime()
        worker = self.f.scripts / "bridgevm-live-worker.sh"
        original = worker.read_text()
        self.assertTrue(original.endswith('main "$@"\n'))
        # Only suppress the daemon entry point; run_job and its helpers stay unchanged.
        self.definitions = self.f.base / "worker-definitions.sh"
        self.definitions.write_text(original.removesuffix('main "$@"\n'))
        self.boundary = self.f.base / "stopped-before-worktree"
        self.f.env.update(BRIDGEVM_REPO=str(self.f.repo), BRIDGEVM_LIVE_WORK=str(self.f.base / "work"),
                          BRIDGEVM_LIVE_MIN_FREE_GIB="0", T12_WORKTREE_BOUNDARY=str(self.boundary))

    def tearDown(self): self.f.close()

    def run_worker(self, job):
        result = subprocess.run(["/bin/bash", "-c", HARNESS, "t12-fixture", str(self.definitions), str(job)],
                                env=self.f.env, capture_output=True, text=True, timeout=20)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.f.phases(), [])
        self.assertFalse(self.f.marker.exists())
        return (job / "result.env").read_text()

    def test_valid_seals_reach_controlled_pre_worktree_stop(self):
        job = self.f.queue_job()
        self.assertEqual(self.run_worker(job), "result=refused-worktree\n")
        self.assertTrue(self.boundary.exists())
        self.assertFalse((self.f.base / "work" / job.name).exists())

    def test_manifest_or_runner_tamper_refuses_before_worktree(self):
        for member in ("input-manifest.tsv", "hvf_gic_boot_probe"):
            with self.subTest(member=member):
                job = self.f.queue_job(member.replace(".", "-"))
                path = job / member; path.chmod(0o600); path.write_bytes(b"changed")
                self.assertEqual(self.run_worker(job), "result=refused-sealed-input\n")
                self.assertFalse(self.boundary.exists())


if __name__ == "__main__": unittest.main()
