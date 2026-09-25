#!/usr/bin/env python3
"""Bounded, single-descriptor B9 guest-share observation contracts."""

from __future__ import annotations

import hashlib
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

REPO = Path(__file__).resolve().parents[2]
MODULES = REPO / "scripts/live-gates"
sys.path.insert(0, str(MODULES))
import b9_raw_capture as capture
from b9_real_workload_observation import read_guest_json

FIFO_CHILD = """
import sys
from pathlib import Path
sys.path.insert(0, sys.argv[1])
from b9_raw_capture import copy_raw
from b9_real_workload_observation import read_guest_json
root, mode = Path(sys.argv[2]), sys.argv[3]
try:
    if mode == 'raw':
        copy_raw(root/'work', root/'raw', root/'share', root/'boot', '0'*32)
    else:
        read_guest_json(root/'boot'/'run.log')
except (OSError, ValueError):
    sys.exit(0)
sys.exit(3)
"""


class GrowingReader:
    def __init__(self, stream, source):
        self.stream, self.source, self.grown = stream, source, False

    def __enter__(self):
        return self

    def __exit__(self, *args):
        return self.stream.__exit__(*args)

    def fileno(self):
        return self.stream.fileno()

    def read(self, size):
        if not self.grown:
            self.grown = True
            with self.source.open("ab") as output:
                output.write(b"unexpected growth")
        return self.stream.read(size)


class GuestShareReopenContract(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory(dir=str(Path(tempfile.gettempdir()).resolve()))
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name).resolve(strict=True)
        for name in ("work", "share", "boot"):
            (self.root / name).mkdir()

    def test_fifo_raw_and_json_refuse_without_writer_or_partial_output(self):
        os.mkfifo(self.root / "boot" / "run.log")
        for mode in ("raw", "json"):
            with self.subTest(mode=mode):
                completed = subprocess.run([sys.executable, "-c", FIFO_CHILD,
                                            str(MODULES), str(self.root), mode],
                                           stdin=subprocess.DEVNULL, capture_output=True,
                                           text=True, timeout=2)
                self.assertEqual(completed.returncode, 0, completed.stderr)
        self.assertFalse((self.root / "raw" / "run.log").exists())

    def test_exact_json_bytes_and_private_copy_digest(self):
        nonce = "0" * 32
        ready = self.root / "share" / ("ready-" + nonce + ".json")
        ready.write_bytes(b'{"nonce":"sealed"}')
        value, digest = read_guest_json(ready)
        self.assertEqual(value, {"nonce": "sealed"})
        self.assertEqual(digest, hashlib.sha256(ready.read_bytes()).hexdigest())
        (self.root / "boot" / "run.log").write_bytes(b"host-log")
        copied = capture.copy_raw(self.root / "work", self.root / "raw",
                                  self.root / "share", self.root / "boot", nonce)
        self.assertEqual(set(copied), {"run.log", "guest-ready.json"})
        for name in copied:
            raw = (self.root / "raw" / name).read_bytes()
            self.assertEqual(copied[name], {"bytes": len(raw),
                                            "sha256": hashlib.sha256(raw).hexdigest()})

    def test_guest_file_over_cap_and_existing_target_refuse(self):
        source = self.root / "share" / "ready.json"
        source.write_bytes(b"oversize")
        target = self.root / "too-large"
        with self.assertRaisesRegex(ValueError, "capture bound"):
            capture._copy_one(source, target, 4)
        self.assertFalse(target.exists())
        target.write_bytes(b"preexisting")
        with self.assertRaises(FileExistsError):
            capture._copy_one(source, target, 100)
        self.assertEqual(target.read_bytes(), b"preexisting")

    def test_growth_removes_only_created_partial_target(self):
        source, target = self.root / "share" / "growing", self.root / "raw-target"
        source.write_bytes(b"two")
        original = os.fdopen

        def growing_fdopen(fd, mode):
            stream = original(fd, mode)
            return GrowingReader(stream, source) if mode == "rb" else stream

        with patch.object(capture.os, "fdopen", growing_fdopen):
            with self.assertRaisesRegex(ValueError, "grew during copy"):
                capture._copy_one(source, target, 8)
        self.assertFalse(target.exists())


if __name__ == "__main__":
    unittest.main()
