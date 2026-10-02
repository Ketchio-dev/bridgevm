#!/usr/bin/env python3
"""Preserve retention paths replaced at cleanup's checked mutation boundaries."""
from __future__ import annotations

import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/live-gates"))
import retained_windows_publication as PUBLICATION  # noqa: E402
import retained_windows_cleanup as CLEANUP  # noqa: E402
from retained_windows_publication import OwnedRetention  # noqa: E402
from retained_windows_cleanup import clear_directory  # noqa: E402


class RetainedWindowsCleanupRacesContract(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="bridgevm-retention-cleanup-")
        self.root = Path(self.temporary.name)
        self.staging = self.root / ".staging"
        self.staging.mkdir(mode=0o700)
        (self.staging / "own-file").write_bytes(b"owned bytes")
        self.owned = OwnedRetention(self.staging)

    def tearDown(self):
        self.temporary.cleanup()

    def test_cleanup_descriptor_stays_on_owned_inode_after_name_replacement(self):
        real_clear = clear_directory
        moved = self.root / "moved-owned-staging"
        replacements = []

        def clear_after_replacement(descriptor):
            opened = self.owned.path
            opened.rename(moved)
            opened.mkdir()
            replacement = opened / "personal"
            replacement.write_bytes(b"keep replacement data")
            replacements.append(replacement)
            return real_clear(descriptor)

        with mock.patch.object(PUBLICATION, "clear_directory", side_effect=clear_after_replacement):
            self.assertFalse(self.owned.cleanup())
        self.assertEqual(replacements[0].read_bytes(), b"keep replacement data")
        self.assertTrue(moved.is_dir())
        self.assertEqual(list(moved.iterdir()), [])

    def test_replacement_between_cleanup_admission_and_rename_is_preserved(self):
        real_rename = PUBLICATION.rename_exclusive
        moved = self.root / "moved-original"
        personal = self.staging / "personal"

        def rename_after_replacement(source, destination):
            if source == self.staging:
                source.rename(moved)
                source.mkdir()
                personal.write_bytes(b"keep replacement tree")
            real_rename(source, destination)

        with mock.patch.object(PUBLICATION, "rename_exclusive", side_effect=rename_after_replacement):
            self.assertFalse(self.owned.cleanup())
        self.assertEqual(personal.read_bytes(), b"keep replacement tree")
        self.assertEqual((moved / "own-file").read_bytes(), b"owned bytes")

    def test_child_replaced_between_listing_and_open_is_never_cleared(self):
        child = self.staging / "child"
        child.mkdir()
        (child / "owned").write_bytes(b"owned")
        moved = self.staging / "moved-child"
        real_open = os.open

        def open_replacement(path, *arguments, **options):
            if path == "child":
                child.rename(moved)
                child.mkdir()
                (child / "personal").write_bytes(b"keep new child")
            return real_open(path, *arguments, **options)

        descriptor = real_open(self.staging, os.O_RDONLY | os.O_DIRECTORY)
        try:
            with mock.patch.object(CLEANUP.os, "open", side_effect=open_replacement):
                with self.assertRaises(OSError):
                    clear_directory(descriptor)
        finally:
            os.close(descriptor)
        self.assertEqual((child / "personal").read_bytes(), b"keep new child")
        self.assertEqual((moved / "owned").read_bytes(), b"owned")


if __name__ == "__main__":
    unittest.main()
