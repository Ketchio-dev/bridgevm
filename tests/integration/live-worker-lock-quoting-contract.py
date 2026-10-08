#!/usr/bin/env python3
"""Queue spellings remain data in the real worker's owned lock EXIT trap."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

LIVE = Path(__file__).resolve().parents[2] / "scripts/live-gates"


class LockQuoting(unittest.TestCase):
    def test_fenced_and_empty_workers_remove_only_their_literal_lock(self):
        with tempfile.TemporaryDirectory(prefix="worker-lock-quote-") as temporary:
            root = Path(temporary).resolve()
            for name in ("queue with spaces", "queue's apostrophe", 'queue"double',
                         "queue'$(touch INJECTED)'suffix", "queue;touch INJECTED"):
                for fenced in (True, False):
                    with self.subTest(name=name, fenced=fenced):
                        queue = root / name; queue.mkdir(mode=0o700)
                        for state in ("queued", "running", "done"):
                            (queue / state).mkdir(mode=0o700)
                        work = root / "work"; work.mkdir(exist_ok=True)
                        fence = queue / "worker-cleanup-required"
                        if fenced: fence.write_text("owned fixture fence\n")
                        unrelated = root / "unrelated"; unrelated.mkdir(exist_ok=True)
                        keep = unrelated / "keep"; keep.write_text("untouched\n")
                        env = dict(os.environ, BRIDGEVM_REPO=str(LIVE.parents[1]),
                                   BRIDGEVM_LIVE_ROOT=str(queue), BRIDGEVM_LIVE_WORK=str(work),
                                   BRIDGEVM_LIVE_MIN_FREE_GIB="0")
                        result = subprocess.run(["/bin/bash", str(LIVE / "bridgevm-live-worker.sh")],
                                                env=env, cwd=root, capture_output=True, text=True, timeout=10)
                        self.assertEqual(result.returncode, 126 if fenced else 0, result.stdout + result.stderr)
                        self.assertFalse((queue / "worker.lock").exists())
                        self.assertFalse((root / "INJECTED").exists())
                        self.assertEqual(keep.read_text(), "untouched\n")
                        if fenced:
                            self.assertEqual(fence.read_text(), "owned fixture fence\n")
                            fence.unlink()
                        expected = ["done", "queued", "running"] if fenced else ["done", "job-ledger", "queued", "running"]
                        self.assertEqual(sorted(p.name for p in queue.iterdir()), expected)
                        for state in expected: (queue / state).rmdir()
                        queue.rmdir()


if __name__ == "__main__": unittest.main()
