#!/usr/bin/env python3
"""Real CLI directory admission under permissive umasks, without a live worker."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
SCRIPTS = ROOT / "scripts/live-gates"


class QueueStorage(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="queue-storage-")
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name).resolve()
        self.queue = self.base / "queue"

    def status(self, mask="022"):
        env = dict(os.environ, BRIDGEVM_LIVE_ROOT=str(self.queue))
        return subprocess.run(["/bin/bash", "-c", 'umask "$1"; exec "$2" status',
                               "fixture", mask, str(SCRIPTS / "bridgevm-live")],
                              env=env, text=True, capture_output=True, timeout=10)

    def test_cli_establishes_private_layout_independent_of_umask(self):
        for mask in ("022", "002"):
            self.queue = self.base / ("queue-" + mask)
            result = self.status(mask)
            self.assertEqual(result.returncode, 0, result.stderr)
            for path in (self.queue, *(self.queue / s for s in ("queued", "running", "done", "job-ledger"))):
                self.assertEqual(path.stat().st_mode & 0o777, 0o700)
            running = self.queue / "running"; running.chmod(0o775)
            self.assertEqual(self.status(mask).returncode, 0)
            self.assertEqual(running.stat().st_mode & 0o777, 0o700)

    def test_queue_and_state_leaf_symlinks_are_not_chmodded_or_adopted(self):
        other = self.base / "other"; other.mkdir(mode=0o755)
        for state in (None, "running", "job-ledger"):
            self.queue = self.base / ("queue-" + str(state))
            if state is None:
                self.queue.symlink_to(other, target_is_directory=True)
            else:
                self.queue.mkdir(mode=0o700)
                (self.queue / state).symlink_to(other, target_is_directory=True)
            self.assertNotEqual(self.status().returncode, 0)
            self.assertEqual(other.stat().st_mode & 0o777, 0o755)
            self.assertEqual(list(other.iterdir()), [])

    def test_raw_terminal_aliases_preserve_target(self):
        other = self.base / "other"; other.mkdir(mode=0o755)
        link = self.base / "link"; link.symlink_to(other, target_is_directory=True)
        for spelling in (str(link) + "/", str(link) + "///", str(link) + "/.", str(link) + "/.."):
            self.queue = spelling
            self.assertNotEqual(self.status().returncode, 0)
            self.assertEqual(other.stat().st_mode & 0o777, 0o755)
            self.assertEqual(list(other.iterdir()), [])

    def test_invalid_dispatch_stops_before_tier_private_output(self):
        output = self.base / "not-a-queue"; output.mkdir(mode=0o700)
        result = subprocess.run([str(SCRIPTS / "run-special-tier.sh"),
                                 "t17-windows-hvf-product-e2e", str(output),
                                 "job", str(self.base / "unused-manifest")],
                                text=True, capture_output=True, timeout=10)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("queued retained-source placement refused", result.stderr)
        self.assertEqual(list(output.iterdir()), [])


if __name__ == "__main__": unittest.main()
