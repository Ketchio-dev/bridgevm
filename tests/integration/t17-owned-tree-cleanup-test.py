#!/usr/bin/env python3
"""Deterministic clone-only cleanup contracts; no VM or private media."""
import importlib.util
import os
from pathlib import Path
import shutil
import stat
import subprocess
import sys
import tempfile
import unittest
from unittest import mock
import uuid


SCRIPT = Path(__file__).resolve().parents[2] / "scripts/live-gates/t17_owned_tree_cleanup.py"
SPEC = importlib.util.spec_from_file_location("t17_owned_tree_cleanup", SCRIPT)
CLEANUP = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CLEANUP)


class OwnedTreeCleanupTests(unittest.TestCase):
    def setUp(self):
        self.job = "cleanup-test-" + uuid.uuid4().hex
        self.root = Path("/tmp") / ("bridgevm-e2e-" + self.job + "." + uuid.uuid4().hex[:6])
        self.root.mkdir(mode=0o700)
        info = self.root.stat()
        self.captured = f"{info.st_dev}:{info.st_ino}"
        self.external = Path(tempfile.mkdtemp(prefix="bridgevm-cleanup-sentinel-"))

    def tearDown(self):
        for root in (self.root, self.external):
            if root.is_symlink():
                root.unlink()
            elif root.exists():
                for directory, _, _ in os.walk(root, followlinks=False):
                    os.chmod(directory, 0o700)
                shutil.rmtree(root)

    def run_cleanup(self):
        CLEANUP.cleanup(str(self.root), self.job, self.captured)

    def make_readonly_payload(self, root):
        payload = root / "payload"
        payload.mkdir()
        for name in ("network", "storage", "serial"):
            directory = payload / name
            directory.mkdir()
            file = directory / "driver.inf"
            file.write_bytes(b"immutable driver fixture\n")
            file.chmod(0o400)
            directory.chmod(0o500)
        payload.chmod(0o500)
        return payload

    def test_readonly_clone_removed_without_changing_source(self):
        source = self.make_readonly_payload(self.external)
        shutil.copytree(source, self.root / "payload")
        self.run_cleanup()
        self.assertFalse(self.root.exists())
        self.assertEqual(stat.S_IMODE(source.stat().st_mode), 0o500)
        for name in ("network", "storage", "serial"):
            directory = source / name
            file = directory / "driver.inf"
            self.assertEqual(stat.S_IMODE(directory.stat().st_mode), 0o500)
            self.assertEqual(stat.S_IMODE(file.stat().st_mode), 0o400)
            self.assertEqual(file.read_bytes(), b"immutable driver fixture\n")

    def test_symlink_rejected_before_any_directory_permissions_change(self):
        payload = self.make_readonly_payload(self.root)
        (self.root / "outside").symlink_to(self.external, target_is_directory=True)
        with self.assertRaises(ValueError):
            self.run_cleanup()
        self.assertEqual(stat.S_IMODE(payload.stat().st_mode), 0o500)
        self.assertTrue((payload / "network/driver.inf").exists())
        self.assertTrue(self.external.is_dir())

    def test_hard_link_rejected_without_changing_external_file(self):
        outside = self.external / "canonical.inf"
        outside.write_bytes(b"canonical")
        outside.chmod(0o400)
        os.link(outside, self.root / "hard-link.inf")
        with self.assertRaises(ValueError):
            self.run_cleanup()
        self.assertEqual(outside.read_bytes(), b"canonical")
        self.assertEqual(stat.S_IMODE(outside.stat().st_mode), 0o400)
        self.assertEqual(outside.stat().st_nlink, 2)

    def test_special_file_rejected(self):
        os.mkfifo(self.root / "pipe")
        with self.assertRaises(ValueError):
            self.run_cleanup()
        self.assertTrue(self.root.exists())

    def test_wrong_job_and_nonexact_root_rejected(self):
        for root, job in ((str(self.root), "another-job"),
                          (str(self.root) + "/", self.job),
                          (str(self.root) + "/../" + self.root.name, self.job),
                          (str(self.external), self.job)):
            with self.subTest(root=root, job=job), self.assertRaises(ValueError):
                CLEANUP.cleanup(root, job, self.captured)
        self.assertTrue(self.root.is_dir())

    def test_wrong_captured_inode_and_missing_identity_rejected(self):
        device, inode = map(int, self.captured.split(":"))
        for captured in (f"{device}:{inode + 1}", "", "unbound"):
            with self.subTest(captured=captured), self.assertRaises(ValueError):
                CLEANUP.cleanup(str(self.root), self.job, captured)
        self.assertTrue(self.root.is_dir())

    def test_different_owner_rejected(self):
        with mock.patch.object(CLEANUP.os, "geteuid", return_value=os.geteuid() + 1):
            with self.assertRaises(ValueError):
                self.run_cleanup()
        self.assertTrue(self.root.is_dir())

    def test_different_device_rejected(self):
        device, inode = map(int, self.captured.split(":"))
        with self.assertRaises(ValueError):
            CLEANUP.cleanup(str(self.root), self.job, f"{device + 1}:{inode}")
        self.assertTrue(self.root.is_dir())

    def test_root_symlink_rejected(self):
        self.root.rmdir()
        self.root.symlink_to(self.external, target_is_directory=True)
        with self.assertRaises(ValueError):
            self.run_cleanup()
        self.assertTrue(self.root.is_symlink())
        self.assertTrue(self.external.is_dir())

    def test_directory_replacement_between_stat_and_open_rejected(self):
        payload = self.make_readonly_payload(self.root)
        original_open = os.open
        replaced = False

        def replace_before_open(path, flags, *args, **kwargs):
            nonlocal replaced
            if path == "payload" and not replaced:
                replaced = True
                payload.rename(self.root / "retained-original")
                payload.mkdir(mode=0o700)
            return original_open(path, flags, *args, **kwargs)

        with mock.patch.object(CLEANUP.os, "open", side_effect=replace_before_open):
            with self.assertRaises(ValueError):
                self.run_cleanup()
        self.assertTrue(replaced)
        original = self.root / "retained-original"
        self.assertEqual(stat.S_IMODE(original.stat().st_mode), 0o500)
        self.assertTrue((original / "network/driver.inf").exists())

    def test_failure_to_make_directory_writable_is_not_success(self):
        payload = self.make_readonly_payload(self.root)
        with mock.patch.object(CLEANUP.os, "fchmod", side_effect=PermissionError("denied")):
            with self.assertRaises(PermissionError):
                self.run_cleanup()
        self.assertTrue((payload / "network/driver.inf").exists())

    def test_cli_removes_readonly_tree(self):
        self.make_readonly_payload(self.root)
        result = subprocess.run(
            [sys.executable, str(SCRIPT), "--root", str(self.root),
             "--job-id", self.job, "--identity", self.captured],
            capture_output=True, text=True, timeout=10)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse(self.root.exists())


if __name__ == "__main__":
    unittest.main()
