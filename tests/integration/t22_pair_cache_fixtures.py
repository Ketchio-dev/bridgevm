"""Shared owned cache fixtures; never a production import root."""
import hashlib
import os
from pathlib import Path
import py_compile
import struct
import subprocess
import unittest

from t22_pair_worker_fixtures import WorkerFixture, LIVE, TIER


class CacheFixture(unittest.TestCase):
    def setUp(self):
        self.fixture = WorkerFixture(self)
        self.directory = self.fixture.sealed_job(state="running", mode="success")
        self.fixture.job(name="z-next")
        self.worktree = self.fixture.sealed_worktree()
        self.marker = self.fixture.root / "cached-code-executed"

    def inventory(self, root=None):
        return subprocess.run(["/bin/bash", "--noprofile", "--norc", "-p",
            str(LIVE / "t22-pair-cache-admission.sh"), str(root or self.worktree)],
            env=self.fixture.env(), capture_output=True, text=True, timeout=35)

    def cache(self, relative):
        target = self.worktree / relative
        original, info = target.read_bytes(), target.stat()
        evil = (f"from pathlib import Path\nPath({str(self.marker)!r}).write_text('cached')\nraise SystemExit(0)\n").encode()
        self.assertLess(len(evil), len(original))
        target.write_bytes(evil + b" " * (len(original) - len(evil)))
        os.utime(target, ns=(info.st_atime_ns, info.st_mtime_ns))
        cached = Path(py_compile.compile(str(target), doraise=True))
        target.write_bytes(original); os.utime(target, ns=(info.st_atime_ns, info.st_mtime_ns))
        self.assertEqual(hashlib.sha256(target.read_bytes()).digest(), hashlib.sha256(original).digest())
        dirty = subprocess.check_output(["/usr/bin/git", "-C", str(self.worktree), "status", "--porcelain", "--untracked-files=all"])
        self.assertEqual(dirty, b"")
        _, flags, timestamp, size = struct.unpack("<IIII", cached.read_bytes()[:16])
        self.assertEqual((flags, timestamp, size), (0, int(info.st_mtime) & 0xffffffff, len(original)))

    def guard(self, expected=126):
        command = ['source "$1"; bridgevm_t17_guard_or_fence "$2" "$3" "$4" "$5" a-fixture "$6"',
            "_", str(self.fixture.repo / "scripts/live-gates/t17-worker-cleanup-fence.sh"), TIER,
            str(self.directory), str(self.worktree), self.fixture.commit, str(self.fixture.queue)]
        result = subprocess.run(["/bin/bash", "--noprofile", "--norc", "-p", "-c", *command],
            env=self.fixture.env(), capture_output=True, text=True, timeout=35)
        self.assertEqual(result.returncode, expected, result.stdout + result.stderr)
        if expected == 126: self.fixture.assert_fenced(result.returncode, result.stderr)
        self.assertFalse(self.marker.exists(), "cached code executed before admission")
