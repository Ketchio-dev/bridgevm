#!/usr/bin/env python3
"""Printed queue paths must not change when transported through shell substitution."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

LIVE = Path(__file__).resolve().parents[2] / "scripts/live-gates"


class Transport(unittest.TestCase):
    def test_raw_and_resolved_newlines_refuse_without_touching_either_target(self):
        with tempfile.TemporaryDirectory(prefix="queue-path-transport-") as temporary:
            root = Path(temporary).resolve()
            target = root / "physical\n"; target.mkdir(mode=0o700)
            sibling = root / "physical"; sibling.mkdir(mode=0o755)
            for path in (target, sibling): (path / "worker-cleanup-required").write_text(path.name)
            alias = root / "alias"; alias.symlink_to(target, target_is_directory=True)
            before = {path: (path.stat().st_mode, list(path.iterdir()), (path / "worker-cleanup-required").read_bytes()) for path in (target, sibling)}
            for selected in (target, alias):
                env = dict(os.environ, BRIDGEVM_LIVE_ROOT=str(selected))
                result = subprocess.run([str(LIVE / "bridgevm-live"), "status"], env=env, capture_output=True, timeout=10)
                self.assertNotEqual(result.returncode, 0)
                for path in (target, sibling):
                    self.assertEqual((path.stat().st_mode, list(path.iterdir()), (path / "worker-cleanup-required").read_bytes()), before[path])


if __name__ == "__main__": unittest.main()
