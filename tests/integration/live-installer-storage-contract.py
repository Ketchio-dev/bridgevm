#!/usr/bin/env python3
"""Selected storage survives env-i; malformed config/provider never installs."""
import importlib.util
import os
from pathlib import Path
import plistlib
import subprocess
import sys
import tempfile
import unittest

from live_installer_fixture import InstallerFixture, assert_refusal

ROOT = Path(__file__).resolve().parents[2]
LIVE = ROOT / "scripts/live-gates"
sys.path.insert(0, str(LIVE))
spec = importlib.util.spec_from_file_location("renderer", LIVE / "render-live-launchagent.py")
renderer = importlib.util.module_from_spec(spec); spec.loader.exec_module(renderer)


class StorageInstall(unittest.TestCase):
    def test_special_characters_are_plist_data_and_configured_values_win(self):
        with tempfile.TemporaryDirectory(prefix="launch-storage-") as temporary:
            root = Path(temporary).resolve(); queue = root / "queue & <xml> ' | __HOME__"; queue.mkdir()
            work = root / "work & space"; work.mkdir()
            home = root / "home"; home.mkdir()
            worker = root / "worker"; worker.write_text('#!/bin/bash\nprintf "%s\\n" "$BRIDGEVM_LIVE_ROOT" "$BRIDGEVM_LIVE_WORK" "$BRIDGEVM_LIVE_MIN_FREE_GIB"\n'); worker.chmod(0o700)
            value = renderer.render(LIVE / "com.ketchio.bridgevm-live.plist", str(home), "fixture", str(worker), str(home / "logs"), str(queue), str(work), "123")
            value = plistlib.loads(plistlib.dumps(value))
            env = dict(os.environ, BRIDGEVM_LIVE_ROOT="wrong queue", BRIDGEVM_LIVE_WORK="wrong work", BRIDGEVM_LIVE_MIN_FREE_GIB="0")
            result = subprocess.run(value["ProgramArguments"], env=env, text=True, capture_output=True, timeout=10)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(result.stdout.splitlines(), [str(queue), str(work), "123"])
            self.assertFalse((home / "BridgeVM/live-queue").exists())

    def test_fresh_custom_queue_install_provisions_the_default_work_parent(self):
        spec = importlib.util.spec_from_file_location("launch_fixture", ROOT / "tests/integration/live-installer-launch-contract.py")
        module = importlib.util.module_from_spec(spec); spec.loader.exec_module(module)
        fixture, value = module.LaunchContract.fixture(self, custom=True)
        work = fixture.home / "BridgeVM/live-work"
        self.assertEqual(work.stat().st_mode & 0o777, 0o700)
        self.assertFalse((fixture.home / "BridgeVM/live-queue").exists())
        self.assertIn(f"BRIDGEVM_LIVE_ROOT={fixture.queue.resolve()}", value["ProgramArguments"])
        result = subprocess.run([sys.executable, "-I", "-B", str(LIVE / "live_storage_capacity.py"), "--prepare-work", "--minimum", "0", str(fixture.queue), str(work)], capture_output=True, timeout=10)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_renderer_isolated_startup_ignores_owned_pythonpath_hook(self):
        with tempfile.TemporaryDirectory(prefix="render-isolation-") as temporary:
            root = Path(temporary).resolve(); marker = root / "hook-ran"
            (root / "sitecustomize.py").write_text(f"open({str(marker)!r}, 'w').write('executed')\n")
            command = [sys.executable, "-I", "-B", str(LIVE / "render-live-launchagent.py"), str(LIVE / "com.ketchio.bridgevm-live.plist"), str(root), "fixture", str(root / "worker"), str(root / "logs"), str(root), str(root / "work"), "100"]
            env = dict(os.environ, PYTHONPATH=str(root))
            result = subprocess.run(command, env=env, capture_output=True, timeout=10)
            self.assertEqual(result.returncode, 0, result.stderr); plistlib.loads(result.stdout)
            self.assertFalse(marker.exists())
            command.remove("-I")
            control = subprocess.run(command, env=env, capture_output=True, timeout=10)
            self.assertEqual(control.returncode, 0, control.stderr)
            self.assertTrue(marker.exists())

    def test_invalid_threshold_and_provider_error_refuse_dry_run_without_mutation(self):
        for kind in ("minimum", "provider"):
            fixture = InstallerFixture(LIVE / "install-studio-queue.sh"); self.addCleanup(fixture.close)
            if kind == "minimum": fixture.env["BRIDGEVM_LIVE_MIN_FREE_GIB"] = "invalid"
            else: fixture.stub("python3", "exit 2\n")
            assert_refusal(self, fixture.run(), "storage capacity unavailable or configuration invalid")


if __name__ == "__main__": unittest.main()
