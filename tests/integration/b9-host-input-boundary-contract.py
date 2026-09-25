#!/usr/bin/env python3
"""Synthetic B9 host file and failure-class boundaries; no guest claim."""

from __future__ import annotations

import hashlib
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

REPO = Path(__file__).resolve().parents[2]
MODULES = REPO / "scripts/live-gates"
sys.path.insert(0, str(MODULES))
from b9_failure_classification import classify_failure
from b9_share_asset_integrity import (DIRECT, bounded_bytes, check_shared_assets,
                                      stable_file)

FIFO_CHILD = """
import sys
from pathlib import Path
sys.path.insert(0, sys.argv[1])
from b9_share_asset_integrity import bounded_bytes, stable_file
try:
    if sys.argv[2] == 'stable':
        stable_file(Path(sys.argv[3]), maximum=100)
    else:
        bounded_bytes(Path(sys.argv[3]), 100)
except (OSError, ValueError):
    sys.exit(0)
sys.exit(3)
"""


class HostInputBoundary(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory(dir=str(Path(tempfile.gettempdir()).resolve()))
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name).resolve(strict=True)

    def test_no_writer_fifo_refuses_without_blocking(self):
        fifo = self.root / "no-writer.fifo"
        os.mkfifo(fifo)
        for reader in ("stable", "bounded"):
            with self.subTest(reader=reader):
                completed = subprocess.run(
                    [sys.executable, "-c", FIFO_CHILD, str(MODULES), reader, str(fifo)],
                    stdin=subprocess.DEVNULL, capture_output=True, text=True, timeout=2)
                self.assertEqual(completed.returncode, 0, completed.stderr)

    def test_regular_positive_and_symlink_negative(self):
        plain = self.root / "plain"
        plain.write_bytes(b"verified")
        digest = hashlib.sha256(b"verified").hexdigest()
        self.assertEqual(stable_file(plain, maximum=100), (8, digest))
        self.assertEqual(bounded_bytes(plain, 100), b"verified")
        alias = self.root / "alias"
        alias.symlink_to(plain)
        for reader in (lambda: stable_file(alias), lambda: bounded_bytes(alias, 100)):
            with self.assertRaises(ValueError):
                reader()

    def test_first_share_mismatch_stays_invalid_after_restoration(self):
        source, share = self.root / "source", self.root / "share"
        source.mkdir()
        share.mkdir()
        records, staged = {}, {}
        for key, name in DIRECT.items():
            raw = ("sealed-" + key).encode()
            original, shared = source / name, share / name
            original.write_bytes(raw)
            shared.write_bytes(raw)
            records[key] = (original, hashlib.sha256(raw).hexdigest())
            staged[name] = stable_file(shared)
        check_shared_assets(records, staged, share)
        changed = share / DIRECT["media"]
        saved = changed.read_bytes()
        changed.write_bytes(b"X" + saved[1:])
        with self.assertRaises(ValueError) as mismatch:
            check_shared_assets(records, staged, share)
        changed.write_bytes(saved)
        check_shared_assets(records, staged, share)
        receipt = {"result_class": "GUEST_NOT_READY"}
        self.assertEqual(classify_failure("asset-prelaunch", mismatch.exception, receipt),
                         "INVALID_EVIDENCE")
        changed.unlink()
        with self.assertRaises(OSError) as missing:
            check_shared_assets(records, staged, share)
        for stage in ("asset-prelaunch", "asset-final"):
            self.assertEqual(classify_failure(stage, missing.exception, receipt),
                             "INVALID_EVIDENCE")
        self.assertEqual(classify_failure("boot", ValueError("unrelated"), receipt),
                         "GUEST_NOT_READY")
        self.assertEqual(classify_failure("vlc", ValueError("bad ready"), receipt),
                         "INVALID_EVIDENCE")


if __name__ == "__main__":
    unittest.main()
