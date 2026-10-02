#!/usr/bin/env python3
"""Unlock only the owned inode before Darwin's permission-sensitive rename."""
from __future__ import annotations

import errno
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/live-gates"))
import retained_windows_publication as PUBLICATION  # noqa: E402
import retained_windows_mutation as MUTATION  # noqa: E402


class RetainedWindowsPermissionsContract(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="bridgevm-retention-permissions-")
        self.root = Path(self.temporary.name)
        self.staging = self.root / "staging"
        self.staging.mkdir(mode=0o700)
        (self.staging / "owned").write_bytes(b"owned bytes")
        self.owned = PUBLICATION.OwnedRetention(self.staging)

    def tearDown(self):
        self.temporary.cleanup()

    def test_locked_tree_is_writable_before_a_permission_sensitive_rename(self):
        real_rename = PUBLICATION.rename_exclusive
        child = self.staging / "child"
        child.mkdir()
        (child / "locked").write_bytes(b"nested bytes")
        (child / "locked").chmod(0o400)
        child.chmod(0o500)
        (self.staging / "owned").chmod(0o400)
        self.staging.chmod(0o500)

        def rename_requiring_owner_write(source, destination):
            if source.stat().st_mode & 0o200 == 0:
                raise PermissionError(errno.EACCES, "rename requires directory write", str(source))
            real_rename(source, destination)

        with mock.patch.object(PUBLICATION, "rename_exclusive", side_effect=rename_requiring_owner_write):
            self.assertTrue(self.owned.cleanup(), self.owned.last_cleanup_error)
        self.assertEqual(list(self.root.iterdir()), [])

    def test_replacement_opened_before_unlock_keeps_its_permissions_and_data(self):
        real_open = os.open
        moved = self.root / "moved-owned"

        def open_replacement(path, *arguments, **options):
            if Path(path) == self.staging:
                self.staging.rename(moved)
                self.staging.mkdir()
                (self.staging / "personal").write_bytes(b"keep replacement")
                self.staging.chmod(0o500)
            return real_open(path, *arguments, **options)

        with mock.patch.object(MUTATION.os, "open", side_effect=open_replacement):
            self.assertFalse(self.owned.cleanup())
        self.assertEqual((self.staging / "personal").read_bytes(), b"keep replacement")
        self.assertEqual(self.staging.stat().st_mode & 0o777, 0o500)
        self.assertEqual((moved / "owned").read_bytes(), b"owned bytes")

    def test_name_replacement_after_open_never_unlocks_the_replacement(self):
        real_open = os.open
        moved = self.root / "moved-owned"
        replacement_identity = None

        def open_then_replace(path, *arguments, **options):
            nonlocal replacement_identity
            descriptor = real_open(path, *arguments, **options)
            if Path(path) == self.staging:
                self.staging.rename(moved)
                self.staging.mkdir()
                (self.staging / "personal").write_bytes(b"keep replacement")
                self.staging.chmod(0o500)
                metadata = self.staging.stat()
                replacement_identity = (metadata.st_dev, metadata.st_ino)
            return descriptor

        with mock.patch.object(MUTATION.os, "open", side_effect=open_then_replace):
            self.assertFalse(self.owned.cleanup())
        replacement = next(path for path in self.root.iterdir()
                           if (path.stat().st_dev, path.stat().st_ino) == replacement_identity)
        self.assertEqual((replacement / "personal").read_bytes(), b"keep replacement")
        self.assertEqual(replacement.stat().st_mode & 0o777, 0o500)
        self.assertEqual((moved / "owned").read_bytes(), b"owned bytes")


if __name__ == "__main__":
    unittest.main()
