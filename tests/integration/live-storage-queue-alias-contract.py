#!/usr/bin/env python3
"""Keep established private queue aliases through worker and installer admission."""
import os
from pathlib import Path
import plistlib
import shutil
import subprocess
import unittest

from live_installer_fixture import InstallerFixture

LIVE = Path(__file__).resolve().parents[2] / "scripts/live-gates"


class QueueAlias(unittest.TestCase):
    def fixture(self):
        f = InstallerFixture(LIVE / "install-studio-queue.sh"); self.addCleanup(f.close)
        physical = f.root / "physical queue"; physical.mkdir(mode=0o700)
        f.queue.symlink_to(physical, target_is_directory=True)
        return f, physical

    def test_installer_freezes_existing_private_alias_as_canonical_queue(self):
        f, physical = self.fixture()
        f.template.write_bytes((LIVE / "com.ketchio.bridgevm-live.plist").read_bytes())
        for name in ("mkdir", "chmod", "id", "bash"):
            p = f.bin / name; p.unlink(missing_ok=True); p.symlink_to(shutil.which(name))
        for name in ("plutil", "launchctl"): f.stub(name, "exit 0\n")
        result = subprocess.run(["/bin/bash", str(f.installer)], env=f.env, capture_output=True, timeout=10)
        self.assertEqual(result.returncode, 0, result.stderr)
        value = plistlib.loads((f.home / "Library/LaunchAgents/com.ketchio.bridgevm-live.plist").read_bytes())
        self.assertIn("BRIDGEVM_LIVE_ROOT=" + str(physical), value["ProgramArguments"])
        self.assertTrue(f.queue.is_symlink())
        self.assertFalse((f.home / "BridgeVM/live-queue").exists())

    def test_actual_worker_preserves_fence_through_alias_and_refuses_nonprivate_target(self):
        f, physical = self.fixture(); fence = physical / "worker-cleanup-required"
        fence.write_text("owned synthetic fence\n")
        work = f.root / "work"; work.mkdir()
        env = dict(os.environ, BRIDGEVM_REPO=str(LIVE.parents[1]), BRIDGEVM_LIVE_ROOT=str(f.queue),
                   BRIDGEVM_LIVE_WORK=str(work), BRIDGEVM_LIVE_MIN_FREE_GIB="0")
        for mode, expected in ((0o700, 126), (0o755, 1)):
            physical.chmod(mode)
            result = subprocess.run(["/bin/bash", str(LIVE / "bridgevm-live-worker.sh")], env=env,
                                    capture_output=True, text=True, timeout=10)
            self.assertEqual(result.returncode, expected, result.stdout + result.stderr)
            self.assertEqual(fence.read_text(), "owned synthetic fence\n")
            self.assertEqual(physical.stat().st_mode & 0o777, mode)
            self.assertEqual(sorted(p.name for p in physical.iterdir()), [fence.name])


if __name__ == "__main__": unittest.main()
