#!/usr/bin/env python3
"""Retention owns its staging inode; concurrent destinations remain untouched."""
from __future__ import annotations

import argparse
import importlib.util
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/live-gates"))
from retained_windows_publication import OwnedRetention  # noqa: E402

SPEC = importlib.util.spec_from_file_location(
    "retain_fixture", ROOT / "scripts/live-gates/retain-windows-import-source.py")
RETAIN = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(RETAIN)


class RetainedWindowsPublicationContract(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="bridgevm-retention-publication-")
        self.root = Path(self.temporary.name)
        self.staging = self.root / ".staging"
        self.staging.mkdir(mode=0o700)
        (self.staging / "own-file").write_bytes(b"owned bytes")
        self.destination = self.root / "retained"
        self.owned = OwnedRetention(self.staging)

    def tearDown(self):
        self.temporary.cleanup()

    def test_success_publishes_and_cleans_only_the_owned_inode(self):
        identity = self.owned.identity
        self.owned.publish(self.destination)
        self.assertFalse(self.staging.exists())
        metadata = self.destination.stat()
        self.assertEqual((metadata.st_dev, metadata.st_ino), identity)
        self.assertEqual((self.destination / "own-file").read_bytes(), b"owned bytes")
        (self.destination / "own-file").chmod(0o400)
        self.destination.chmod(0o500)
        self.assertTrue(self.owned.cleanup())
        self.assertFalse(self.destination.exists())

    def test_every_existing_destination_refuses_publication_and_is_preserved(self):
        for kind in ("empty-directory", "populated-directory", "file", "symlink"):
            with self.subTest(kind=kind):
                destination = self.root / kind
                if kind.endswith("directory"):
                    destination.mkdir()
                    if kind == "populated-directory":
                        (destination / "personal").write_bytes(b"preserve")
                elif kind == "file":
                    destination.write_bytes(b"preserve")
                else:
                    destination.symlink_to(self.staging, target_is_directory=True)
                before = destination.lstat()
                with self.assertRaises(FileExistsError):
                    self.owned.publish(destination)
                after = destination.lstat()
                self.assertEqual((after.st_dev, after.st_ino), (before.st_dev, before.st_ino))
                self.assertTrue((self.staging / "own-file").exists())
        self.assertTrue(self.owned.cleanup())
        self.assertEqual((self.root / "populated-directory/personal").read_bytes(), b"preserve")
        self.assertEqual((self.root / "file").read_bytes(), b"preserve")
        self.assertTrue((self.root / "empty-directory").is_dir())
        self.assertTrue((self.root / "symlink").is_symlink())

    def test_replaced_staging_and_published_names_are_never_cleaned(self):
        for publish in (False, True):
            with self.subTest(publish=publish):
                staging = self.root / f"stage-{publish}"
                staging.mkdir()
                owned = OwnedRetention(staging)
                current = self.root / f"final-{publish}" if publish else staging
                if publish:
                    owned.publish(current)
                moved = self.root / f"moved-{publish}"
                current.rename(moved)
                current.mkdir()
                personal = current / "personal"
                personal.write_bytes(b"preserve replacement")
                self.assertFalse(owned.cleanup())
                self.assertEqual(personal.read_bytes(), b"preserve replacement")
                if not publish:
                    with self.assertRaises(ValueError):
                        owned.publish(self.destination)

    def test_cleanup_never_follows_symlinks_or_chmods_hard_link_aliases(self):
        outside = self.root / "outside"
        outside.mkdir()
        personal = outside / "personal"
        personal.write_bytes(b"outside bytes")
        personal.chmod(0o400)
        (self.staging / "linked-directory").symlink_to(outside, target_is_directory=True)
        (self.staging / "linked-file").symlink_to(personal)
        os.link(personal, self.staging / "hard-link")
        self.assertTrue(self.owned.cleanup())
        self.assertEqual(personal.read_bytes(), b"outside bytes")
        self.assertEqual(personal.stat().st_mode & 0o777, 0o400)

    def test_retention_failure_keeps_a_concurrent_destination_and_cleans_staging(self):
        state = self.root / "state"
        state.mkdir()
        (state / "dummy").write_bytes(b"fixture state")
        app = self.root / "app"
        request = {"job_id": "fixture", "commit": "a" * 40, "campaign_mode": "pilot", "lane": 1,
                   "app_bundle_path": str(app), "app_executable_path": str(app / "executable"),
                   "runner_path": str(app / "runner"), "vtpm_state_path": str(state), "vm_slug": "vm"}
        assets = {key: {"path": str(value), "sha256": "a" * 64} for key, value in (
            ("app_bundle", app), ("app_executable", app / "executable"),
            ("product_helper", app / "helper"), ("runner", app / "runner"))}
        request_path = self.root / "request.json"
        request_path.write_text(json.dumps(request))
        verified_path = self.root / "verified.json"
        verified_path.write_text(json.dumps({"verified": True, "campaign_mode": "pilot", "assets": assets}))
        stamp_path = self.root / "stamp.json"
        stamp_path.write_text(json.dumps({"request_sha256": RETAIN.T17.digest(request_path)}))
        result = {**{stage: True for stage in RETAIN.T17.LANE_STAGES}, "cleanup_verified": True}
        personal = self.destination / "personal"

        def fail_export(*arguments):
            self.destination.mkdir()
            personal.write_bytes(b"concurrent data")
            raise ValueError("injected export failure")

        args = argparse.Namespace(request=request_path, verified=verified_path, stamp=stamp_path,
                                  result=self.root / "unused", destination=self.destination,
                                  status=self.root / "status.json")
        with mock.patch.object(RETAIN.T17, "lane", return_value=result), \
                mock.patch.object(RETAIN, "export_selected", side_effect=fail_export):
            with self.assertRaisesRegex(ValueError, "injected export failure"):
                RETAIN.retain(args)
        self.assertEqual(personal.read_bytes(), b"concurrent data")
        self.assertEqual(list(self.root.glob(".retained.stage-*")), [])
        self.assertFalse(args.status.exists())


if __name__ == "__main__":
    unittest.main()
