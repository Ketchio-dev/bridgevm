"""Deterministic ownership/failure cases; no real host mounts are touched."""
from pathlib import Path
from types import SimpleNamespace
from unittest import TestCase, mock
import plistlib
import signal
import subprocess
import tempfile

import winpe_companion_mounts as mounts


class WinPEMountSafetyTests(TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.parent = Path(self.temp.name).resolve()
        self.image = self.parent / "image.raw"
        self.image.write_bytes(b"private-fixture")
        self.root = self.parent / "mount-before"
        self.entities = [{"dev-entry": "/dev/disk77"},
                         {"dev-entry": "/dev/disk77s1", "mount-point": str(self.root / "Windows")}]
        self.owned = {"image-path": str(self.image), "system-entities": self.entities}
        self.calls = []
        self.inventories = []
        self.attach = SimpleNamespace(returncode=0, stdout=plistlib.dumps({"system-entities": self.entities}))
        self.detach = SimpleNamespace(returncode=0, stdout=b"")

    def command(self, argv, **kwargs):
        self.calls.append(argv)
        self.assertIn(kwargs["timeout"], (10, 20, 60))
        if argv[1] == "info":
            value = self.inventories.pop(0)
            if isinstance(value, BaseException):
                raise value
            return SimpleNamespace(returncode=0, stdout=plistlib.dumps({"images": value}))
        value = self.attach if argv[1] == "attach" else self.detach
        if isinstance(value, BaseException):
            raise value
        return value

    def use(self):
        return mock.patch.object(mounts.subprocess, "run", side_effect=self.command)

    def detached(self):
        return [command[2] for command in self.calls if command[1] == "detach"]

    def failure(self, cleanup):
        with self.assertRaises(mounts.MountSafetyError) as caught:
            with self.use(), mounts.mounted_image(self.image, self.root):
                pass
        self.assertIs(caught.exception.cleanup_complete, cleanup)

    def test_normal_exact_ownership_and_independent_absence(self):
        self.inventories = [[], [self.owned], [self.owned], []]
        with self.use(), mounts.mounted_image(self.image, self.root) as entities:
            self.assertEqual(entities, self.entities)
            self.assertTrue(self.root.is_dir())
        self.assertEqual(self.detached(), ["/dev/disk77"])
        self.assertFalse(self.root.exists())
        self.assertEqual(self.inventories, [])

    def test_nonzero_attach_is_cleaned_using_inventory(self):
        self.attach.returncode = 1
        self.inventories = [[], [self.owned], []]
        self.failure(True)
        self.assertEqual(self.detached(), ["/dev/disk77"])

    def test_malformed_attach_plist_is_cleaned_using_inventory(self):
        self.attach.stdout = b"not a plist"
        self.inventories = [[], [self.owned], []]
        self.failure(True)
        self.assertEqual(self.detached(), ["/dev/disk77"])

    def test_ambiguous_attach_output_does_not_select_arbitrary_device(self):
        self.attach.stdout = plistlib.dumps({"system-entities": [
            {"dev-entry": "/dev/disk77"}, {"dev-entry": "/dev/disk88"}]})
        self.inventories = [[], [self.owned], []]
        self.failure(True)
        self.assertEqual(self.detached(), ["/dev/disk77"])

    def test_preexisting_target_is_never_detached(self):
        self.inventories = [[self.owned]]
        self.failure(False)
        self.assertEqual([call[1] for call in self.calls], ["info"])
        self.assertFalse(self.root.exists())

    def test_other_backing_is_never_detached(self):
        other = {"image-path": str(self.parent / "other.raw"),
                 "system-entities": [{"dev-entry": "/dev/disk88"}]}
        self.attach.returncode = 1
        self.inventories = [[other], [other], [other]]
        self.failure(True)
        self.assertEqual(self.detached(), [])

    def test_foreign_mount_point_refuses_detach(self):
        foreign = {"image-path": str(self.image), "system-entities": [
            {"dev-entry": "/dev/disk77"}, {"dev-entry": "/dev/disk77s1",
             "mount-point": str(self.parent / "not-owned")}]}
        self.inventories = [[], [foreign], [foreign]]
        self.failure(False)
        self.assertEqual(self.detached(), [])
        self.assertTrue(self.root.exists())

    def test_detach_failure_never_reports_clean(self):
        self.detach.returncode = 1
        self.inventories = [[], [self.owned], [self.owned], []]
        self.failure(False)
        self.assertTrue(self.root.exists())

    def test_detach_zero_with_remaining_attachment_is_not_absence(self):
        self.inventories = [[], [self.owned], [self.owned], [self.owned]]
        self.failure(False)
        self.assertTrue(self.root.exists())

    def test_detach_timeout_is_not_absence(self):
        self.detach = subprocess.TimeoutExpired(["hdiutil", "detach"], 20)
        self.inventories = [[], [self.owned], [self.owned]]
        self.failure(False)

    def test_attach_timeout_remains_uncertain_after_observed_absence(self):
        self.attach = subprocess.TimeoutExpired(["hdiutil", "attach"], 60)
        self.inventories = [[], [], []]
        self.failure(False)
        self.assertTrue(self.root.exists())

    def test_attach_timeout_still_cleans_exact_observed_mount(self):
        self.attach = subprocess.TimeoutExpired(["hdiutil", "attach"], 60)
        self.inventories = [[], [self.owned], []]
        self.failure(False)
        self.assertEqual(self.detached(), ["/dev/disk77"])

    def test_interrupted_attach_remains_uncertain_after_observed_absence(self):
        self.attach = InterruptedError("mount-operation-cancelled-15")
        self.inventories = [[], [], []]
        self.failure(False)
        self.assertTrue(self.root.exists())

    def test_keyboard_interrupt_during_attach_is_uncertain(self):
        self.attach = KeyboardInterrupt()
        self.inventories = [[], [], []]
        self.failure(False)
        self.assertTrue(self.root.exists())

    def test_attach_os_error_remains_fail_closed(self):
        self.attach = OSError("attach-process-error")
        self.inventories = [[], [], []]
        self.failure(False)
        self.assertTrue(self.root.exists())

    def test_inventory_failure_prevents_attach(self):
        self.inventories = [subprocess.TimeoutExpired(["hdiutil", "info"], 10)]
        self.failure(False)
        self.assertEqual([call[1] for call in self.calls], ["info"])

    def test_device_reassignment_is_not_detached(self):
        moved = {"image-path": str(self.image), "system-entities": [{"dev-entry": "/dev/disk88"}]}
        self.inventories = [[], [self.owned], [moved]]
        self.failure(False)
        self.assertEqual(self.detached(), [])

    def test_body_exception_is_reported_after_cleanup(self):
        self.inventories = [[], [self.owned], [self.owned], []]
        with self.assertRaises(mounts.MountSafetyError) as caught:
            with self.use(), mounts.mounted_image(self.image, self.root):
                raise ValueError("hash-failed")
        self.assertTrue(caught.exception.cleanup_complete)
        self.assertIsInstance(caught.exception.__cause__, ValueError)
        self.assertFalse(self.root.exists())

    def test_signal_cancellation_cleans_and_restores_handlers(self):
        for sig in (signal.SIGINT, signal.SIGTERM):
            with self.subTest(signal=sig):
                previous = signal.getsignal(sig)
                self.inventories = [[], [self.owned], [self.owned], []]
                with self.assertRaises(mounts.MountSafetyError) as caught:
                    with self.use(), mounts.mounted_image(self.image, self.root):
                        signal.getsignal(sig)(sig, None)
                self.assertTrue(caught.exception.cleanup_complete)
                self.assertIs(signal.getsignal(sig), previous)

    def test_nonempty_private_root_is_retained_not_recursively_deleted(self):
        self.inventories = [[], [self.owned], [self.owned], []]
        with self.assertRaises(mounts.MountSafetyError) as caught:
            with self.use(), mounts.mounted_image(self.image, self.root):
                (self.root / "unexpected").write_bytes(b"preserve")
        self.assertFalse(caught.exception.cleanup_complete)
        self.assertEqual((self.root / "unexpected").read_bytes(), b"preserve")
