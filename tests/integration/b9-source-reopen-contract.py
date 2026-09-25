#!/usr/bin/env python3
"""Bounded B9 source-reopen contracts using only synthetic local files."""

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
import b9_real_workload_inputs as inputs

DIRECT = (("media", "bbb_1080p_10s_5MB_av1.webm"),
          ("presentmon", "PresentMon-2.5.1-x64.exe"),
          ("guest_script", "bv-b9-vlc-playback.ps1"),
          ("guest_helper_script", "bv-b9-private-inputs.ps1"),
          ("control_script", "bv-b9-control.ps1"))

FIFO_CHILD = """
import hashlib, sys
from pathlib import Path
sys.path.insert(0, sys.argv[1])
from b9_real_workload_inputs import _exclusive_copy, stage_share
root, mode = Path(sys.argv[2]), sys.argv[3]
try:
    if mode == 'direct':
        _exclusive_copy(root/'no-writer.fifo', root/'target')
    else:
        records = {}
        for key, name in (('media','bbb_1080p_10s_5MB_av1.webm'),
                          ('presentmon','PresentMon-2.5.1-x64.exe'),
                          ('guest_script','bv-b9-vlc-playback.ps1'),
                          ('guest_helper_script','bv-b9-private-inputs.ps1'),
                          ('control_script','bv-b9-control.ps1')):
            source = root/name
            records[key] = (source, hashlib.sha256(source.read_bytes()).hexdigest())
        records['vlc_zip'] = (root/'no-writer.fifo', '0'*64)
        stage_share(records, root/'share')
except (OSError, ValueError):
    sys.exit(0)
sys.exit(3)
"""


class ReopenContract(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory(dir=str(Path(tempfile.gettempdir()).resolve()))
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name).resolve(strict=True)

    def records(self, archive: Path) -> dict:
        records = {}
        for key, name in DIRECT:
            source = self.root / name
            source.write_bytes(b"x")
            records[key] = (source, hashlib.sha256(b"x").hexdigest())
        records["vlc_zip"] = (archive, hashlib.sha256(archive.read_bytes()).hexdigest())
        return records

    def test_no_writer_fifo_rejected_at_both_reopens(self):
        fifo = self.root / "no-writer.fifo"
        os.mkfifo(fifo)
        for _, name in DIRECT:
            (self.root / name).write_bytes(b"x")
        for mode in ("direct", "archive"):
            with self.subTest(mode=mode):
                result = subprocess.run([sys.executable, "-c", FIFO_CHILD, str(MODULES),
                                         str(self.root), mode], stdin=subprocess.DEVNULL,
                                        capture_output=True, text=True, timeout=2)
                self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse((self.root / "target").exists())

    def test_regular_file_and_tiny_ten_chunk_archive(self):
        source, target = self.root / "source", self.root / "target"
        source.write_bytes(b"regular")
        self.assertEqual(inputs._exclusive_copy(source, target),
                         (7, hashlib.sha256(b"regular").hexdigest()))
        archive = self.root / "vlc.zip"
        archive.write_bytes(b"abcd" * 10)
        with patch.object(inputs, "CHUNK_BYTES", 4):
            staged = inputs.stage_share(self.records(archive), self.root / "share")
        self.assertEqual(len(staged), 16)
        self.assertEqual(b"".join((self.root / "share" / f"b9-vlc-part-{i:02d}.bin").read_bytes()
                                  for i in range(10)), archive.read_bytes())

    def test_symlink_and_hardlink_source_refuse_without_target(self):
        source = self.root / "source"
        source.write_bytes(b"regular")
        for name, link in (("symlink", self.root / "alias-symbolic"),
                           ("hardlink", self.root / "alias-hard")):
            if name == "symlink":
                link.symlink_to(source)
            else:
                os.link(source, link)
            target = self.root / (name + "-target")
            with self.assertRaises(ValueError):
                inputs._exclusive_copy(link, target)
            self.assertFalse(target.exists())
            link.unlink()
        archive = self.root / "vlc.zip"
        archive.symlink_to(source)
        records = self.records(source)
        records["vlc_zip"] = (archive, hashlib.sha256(source.read_bytes()).hexdigest())
        with self.assertRaises(ValueError):
            inputs.stage_share(records, self.root / "share")


if __name__ == "__main__":
    unittest.main()
