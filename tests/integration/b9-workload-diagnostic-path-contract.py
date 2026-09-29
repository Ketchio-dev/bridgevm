#!/usr/bin/env python3
"""Synthetic B9 CSV path identity refusals; never guest workload evidence."""

from __future__ import annotations

import hashlib
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
import b9_workload_diagnostic as diagnostic

CSV = ("Application,ProcessID,SwapChainAddress,CPUStartTime,FrameTime,CPUBusy,CPUWait\r\n"
       "vlc.exe,123,0x20,0.0000,10.0000,10.0000,0\r\n"
       "vlc.exe,123,0x20,10.0000,11.0000,11.0000,0\r\n").encode()


class CsvPathContract(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name)
        self.csv = self.root / "presentmon.csv"
        self.csv.write_bytes(CSV)
        self.digest = hashlib.sha256(CSV).hexdigest()

    def test_ordinary_csv_positive_remains_csv_only(self):
        result = diagnostic.summarize_csv(self.csv, self.digest, 123)
        self.assertEqual(result["diagnostic_class"], "CSV_APP_ROWS_VALID")
        self.assertEqual(result["frame_count"], 2)
        self.assertFalse(result["criterion_pass"])

    def after_open_swap(self, replacement: Path) -> None:
        opened = os.open
        identities = []

        def swap(path, flags, *args, **kwargs):
            fd = opened(path, flags, *args, **kwargs)
            if Path(path) == self.csv and not identities:
                before = os.fstat(fd)
                os.replace(replacement, self.csv)
                after = os.lstat(self.csv)
                identities.append(((before.st_dev, before.st_ino),
                                   (after.st_dev, after.st_ino)))
            return fd

        with patch.object(diagnostic.os, "open", swap):
            with self.assertRaisesRegex(diagnostic.DiagnosticError, "changed"):
                diagnostic.summarize_csv(self.csv, self.digest, 123)
        self.assertNotEqual(*identities[0])

    def test_same_length_replacement_after_descriptor_open_refuses(self):
        replacement = self.root / "different.csv"
        replacement.write_bytes(b"X" + CSV[1:])
        self.assertEqual(replacement.stat().st_size, self.csv.stat().st_size)
        self.after_open_swap(replacement)
        self.assertNotEqual(hashlib.sha256(self.csv.read_bytes()).hexdigest(), self.digest)

    def test_symlink_replacement_after_descriptor_open_refuses(self):
        other = self.root / "other.csv"
        other.write_bytes(CSV)
        replacement = self.root / "alias.csv"
        replacement.symlink_to(other)
        self.after_open_swap(replacement)
        self.assertTrue(self.csv.is_symlink())

    def test_no_writer_fifo_refuses_within_child_timeout(self):
        self.csv.unlink()
        os.mkfifo(self.csv)
        child = """import sys
from pathlib import Path
sys.path.insert(0, sys.argv[1])
from b9_workload_diagnostic import DiagnosticError, bounded_regular_bytes
try: bounded_regular_bytes(Path(sys.argv[2]), 1024)
except DiagnosticError: sys.exit(0)
sys.exit(3)
"""
        result = subprocess.run([sys.executable, "-c", child,
                                 str(ROOT / "scripts"), str(self.csv)],
                                capture_output=True, text=True, timeout=2)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_missing_required_open_flags_refuses(self):
        for missing in ("O_NOFOLLOW", "O_NONBLOCK"):
            with self.subTest(missing=missing):
                class MissingFlagOS:
                    def __getattr__(self, name):
                        if name == missing: raise AttributeError(name)
                        return getattr(os, name)
                with patch.object(diagnostic, "os", MissingFlagOS()), self.assertRaises(diagnostic.DiagnosticError) as caught:
                    diagnostic.bounded_regular_bytes(self.csv, 1024)
                self.assertEqual(str(caught.exception.__cause__), missing)


if __name__ == "__main__":
    unittest.main(verbosity=2)
