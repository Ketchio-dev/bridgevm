#!/usr/bin/env python3
"""Queued retention cannot escape the owned queue or cross a mounted device."""
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "scripts/live-gates"))
import product_e2e_retained_destination as retained


class RetainedDestinationTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="retained-destination-")
        self.queue = Path(self.temp.name).resolve()
        self.queue.chmod(0o700)
        self.running = self.queue / "running"; self.running.mkdir(mode=0o755)
        self.output = self.running / "job"; self.output.mkdir(mode=0o700)
        self.parent = self.queue / "t19-sources"

    def tearDown(self): self.temp.cleanup()

    def test_exact_destination_and_no_overwrite(self):
        target = retained.destination(self.output, "job")
        self.assertEqual(target, self.parent / "job")
        self.assertEqual(self.parent.stat().st_mode & 0o777, 0o700)
        target.mkdir()
        with self.assertRaises(ValueError): retained.destination(self.output, "job")

    def test_wrong_job_parent_permissions_and_symlink_refuse(self):
        for path, job in ((self.output, "other"), (self.queue, "job"), (self.output, "../job")):
            with self.assertRaises(ValueError): retained.destination(path, job)
        self.parent.symlink_to(self.running, target_is_directory=True)
        with self.assertRaises(ValueError): retained.destination(self.output, "job")
        self.parent.unlink(); self.queue.chmod(0o777)
        with self.assertRaises(ValueError): retained.destination(self.output, "job")
        self.queue.chmod(0o700)

    def test_separate_retention_device_refuses(self):
        self.parent.mkdir(mode=0o700)
        actual = retained.directory
        def directory(path, private=True):
            value = actual(path, private)
            return value + 1 if path == self.parent else value
        with patch.object(retained, "directory", side_effect=directory):
            with self.assertRaises(ValueError): retained.destination(self.output, "job")
        self.assertFalse((self.parent / "job").exists())


if __name__ == "__main__": unittest.main()
