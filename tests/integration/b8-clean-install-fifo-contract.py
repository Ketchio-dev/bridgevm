#!/usr/bin/env python3
"""Bounded refusal of named pipes by the offline B8 source readers."""

from __future__ import annotations

import hashlib
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "scripts/live-gates"))
import b8_clean_install_files as files

CHILD = """\
from pathlib import Path
import sys
sys.path.insert(0, sys.argv[1])
import b8_clean_install_files as files
source, destination = Path(sys.argv[2]), Path(sys.argv[3])
try:
    if sys.argv[4] == 'read':
        files.read_regular(source, 1024)
    elif sys.argv[4] == 'hash':
        files._hash(source, 1024)
    else:
        files._copy_tar(source, destination, '0' * 64, 1024)
except ValueError:
    print('REFUSED')
    raise SystemExit(0)
raise SystemExit('nonregular B8 source was accepted')
"""


class FifoSourceContract(unittest.TestCase):
    def test_no_writer_fifo_is_bounded_for_all_readers(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve(strict=True)
            source, destination = root / "no-writer.pipe", root / "out.tar"
            os.mkfifo(source, 0o600)
            for mode in ("read", "hash", "copy"):
                with self.subTest(mode=mode):
                    try:
                        result = subprocess.run(
                            [sys.executable, "-c", CHILD,
                             str(REPO / "scripts/live-gates"), str(source),
                             str(destination), mode],
                            capture_output=True, text=True, timeout=2, check=False)
                    except subprocess.TimeoutExpired as error:
                        self.fail(f"B8 {mode} blocked opening a FIFO: {error}")
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertEqual(result.stdout.strip(), "REFUSED")
                    self.assertFalse(destination.exists())

    def test_regular_source_is_unchanged(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve(strict=True)
            source, destination = root / "input.tar", root / "out.tar"
            raw = b"exact bounded B8 source bytes"
            source.write_bytes(raw)
            expected = hashlib.sha256(raw).hexdigest()
            self.assertEqual(files.read_regular(source, 1024), raw)
            self.assertEqual(files._hash(source, 1024), expected)
            files._copy_tar(source, destination, expected, 1024)
            self.assertEqual(destination.read_bytes(), raw)


if __name__ == "__main__":
    unittest.main()
