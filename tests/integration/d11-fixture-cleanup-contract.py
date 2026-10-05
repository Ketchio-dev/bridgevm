#!/usr/bin/env python3
"""Owned process/mount transitions and service gating with mocked providers."""
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import Mock, patch

from d11_fixture_test_support import HASH
from d11_fixture_process import Processes
from d11_fixture_mounts import Container, device
from d11_fixture_guest import FixtureController
from d11_fixture_commands import stage_probe
from d11_fixture_files import FileSeal, digest, identity


class Cleanup(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(); self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        self.processes = Processes(self.root, self.root, self.root)

    def test_spawn_failure_cannot_be_erased_by_second_cleanup(self):
        self.processes.attempted = True
        with self.assertRaises(ValueError): self.processes.finish()
        self.processes.finish()
        self.assertFalse(self.processes.cleanup_complete)

    def test_exited_leader_with_live_group_remains_unclean(self):
        child = Mock(pid=1234, returncode=0)
        self.processes.process = child; self.processes.attempted = True
        with patch("d11_fixture_process.stop", return_value=False):
            with self.assertRaises(ValueError): self.processes.finish()
        self.assertFalse(self.processes.cleanup_complete)

    def test_terminal_and_absent_group_is_clean(self):
        child = Mock(pid=1234, returncode=0)
        self.processes.process = child; self.processes.attempted = True
        with patch("d11_fixture_process.stop", return_value=True), patch("d11_fixture_process.state", return_value="absent"):
            self.processes.finish()
        self.assertTrue(self.processes.cleanup_complete)
        self.assertEqual(self.processes.records[0]["exit_code"], 0)

    def test_cancel_checked_before_launch(self):
        (self.root / "cancel.requested").touch()
        with patch("d11_fixture_process.subprocess.Popen") as launch:
            with self.assertRaises(InterruptedError): self.processes.launch(["unused"], self.root / "log")
        launch.assert_not_called()

    def test_mount_ownership_ignores_unrelated_iso_mount(self):
        box = Container(self.root, self.root / "source.iso", None, 24)
        foreign = {"image-path": str(box.iso), "system-entities": [{"mount-point": "/Volumes/Unrelated"}]}
        self.assertFalse(box.belongs(foreign))
        foreign["system-entities"][0]["mount-point"] = str(box.mount / "tmp/iso")
        self.assertTrue(box.belongs(foreign))
        self.assertTrue(box.belongs({"image-path": str(box.mount / "target.raw")}))

    def test_whole_device_selection_includes_iso_without_guid_hint(self):
        self.assertEqual(device({"system-entities": [{"dev-entry": "/dev/disk80"}]}), "/dev/disk80")
        with self.assertRaises(ValueError): device({"system-entities": [{"dev-entry": "/dev/disk80s1"}]})

    def test_nested_detach_precedes_container_and_never_forces(self):
        box = Container(self.root, self.root / "source.iso", None, 24)
        def row(path, number): return {"image-path": str(path), "system-entities": [{"dev-entry": "/dev/disk" + str(number)}]}
        mounts = [row(box.backing, 80), row(box.mount / "target.raw", 81)]
        child = Mock(returncode=0); child.wait.return_value = 0
        with patch("d11_fixture_mounts.inventory", side_effect=[mounts, []]), \
             patch("d11_fixture_mounts.subprocess.Popen", return_value=child) as launch, \
             patch("guest_input_owned_group.stop", return_value=True):
            self.assertTrue(box.cleanup())
        self.assertEqual([c.args[0] for c in launch.call_args_list],
                         [["/usr/bin/hdiutil", "detach", "/dev/disk81"], ["/usr/bin/hdiutil", "detach", "/dev/disk80"]])

    def test_detach_failure_is_not_clean(self):
        box = Container(self.root, self.root / "source.iso", None, 24)
        row = {"image-path": str(box.backing), "system-entities": [{"dev-entry": "/dev/disk80"}]}
        child = Mock(returncode=1); child.wait.return_value = 1
        with patch("d11_fixture_mounts.inventory", return_value=[row]), \
             patch("d11_fixture_mounts.subprocess.Popen", return_value=child), \
             patch("guest_input_owned_group.stop", return_value=True):
            with self.assertRaises(ValueError): box.cleanup()

    def test_partial_attach_failed_nested_detach_preserves_backing(self):
        box = Container(self.root, self.root / "source.iso", None, 24)
        box.backing.write_bytes(b"owned synthetic backing")
        box.backing_identity = identity(box.backing.lstat())[:2]
        iso = {"image-path": str(box.iso), "system-entities": [{"dev-entry": "/dev/disk82", "mount-point": str(box.mount / "tmp/iso")}]}
        source = {"image-path": str(box.mount / "source.raw"), "system-entities": [{"dev-entry": "/dev/disk81"}]}
        backing = {"image-path": str(box.backing), "system-entities": [{"dev-entry": "/dev/disk80"}]}
        child = Mock(returncode=1); child.wait.return_value = 1
        with patch("d11_fixture_mounts.inventory", return_value=[backing, iso, source]), \
             patch("d11_fixture_mounts.subprocess.Popen", return_value=child) as launch, \
             patch("guest_input_owned_group.stop", return_value=True):
            with self.assertRaises(ValueError): box.cleanup()
        self.assertEqual(box.backing.read_bytes(), b"owned synthetic backing")
        self.assertEqual(launch.call_args.args[0], ["/usr/bin/hdiutil", "detach", "/dev/disk82"])

    def test_staged_executable_seal_detects_copy_mutation(self):
        binary = self.root / "sealed-binary"; binary.write_bytes(b"synthetic executable")
        root = self.root / "worktree"; root.mkdir()
        staged = stage_probe(root, binary, digest(binary))
        with FileSeal(staged, 1024, digest(binary)) as seal:
            staged.chmod(0o700); staged.write_bytes(b"changed executable")
            with self.assertRaises(ValueError): seal.check()

    def test_symlinked_staging_parent_cannot_create_unowned_directories(self):
        binary = self.root / "sealed-binary"; binary.write_bytes(b"synthetic executable")
        root = self.root / "worktree"; root.mkdir()
        foreign = self.root / "foreign"; foreign.mkdir()
        (root / "target").symlink_to(foreign, target_is_directory=True)
        with self.assertRaises(ValueError): stage_probe(root, binary, digest(binary))
        self.assertEqual(list(foreign.iterdir()), [])

    def test_control_requires_both_ready_and_service(self):
        control = self.root / "agent.ctl"; control.touch()
        log = self.root / "run.log"; log.write_text("BVAGENT READY\r\n")
        driver = FixtureController(control, log, self.root, "d" * 64, HASH, Mock())
        with self.assertRaises(ValueError): driver.write_command("shutdown.exe /s /t 0")
        self.assertEqual(control.read_bytes(), b"")
        log.write_text("BVAGENT READY\r\nBVAGENT SERVICE start\r\n")
        driver.write_command("shutdown.exe /s /t 0")
        self.assertEqual(control.read_bytes(), b"shutdown.exe /s /t 0\n")


if __name__ == "__main__": unittest.main()
