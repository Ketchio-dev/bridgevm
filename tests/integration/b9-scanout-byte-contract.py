#!/usr/bin/env python3
"""Bind synthetic B9 scanout bytes to the exact parsed and hashed descriptor."""

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
import b9_share_asset_integrity as integrity
from b9_real_workload_observation import scanout_hashes

FIFO_CHILD = """
import sys
from pathlib import Path
sys.path.insert(0, sys.argv[1])
from b9_real_workload_observation import scanout_hashes
try:
    scanout_hashes([Path(sys.argv[2])/'presented.ppm'])
except (OSError, ValueError):
    sys.exit(0)
sys.exit(3)
"""


class ScanoutByteContract(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory(dir=str(Path(tempfile.gettempdir()).resolve()))
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name).resolve(strict=True)
        width, height = 640, 480
        bgra = bytes((0, 0, 255, 255)) * (width * height)
        ppm = f"P6\n{width} {height}\n255\n".encode() + bytes((255, 0, 0)) * (width * height)
        capture = ("source=active-cgl-iosurface\n"
                   "iosurface_id=1\n"
                   f"width={width}\nheight={height}\n"
                   "initial_seed=1\ncaptured_seed=2\n"
                   f"nonblack_pixels={width * height}\n"
                   f"bgra_sha256={hashlib.sha256(bgra).hexdigest()}\n"
                   f"ppm_sha256={hashlib.sha256(ppm).hexdigest()}\n").encode()
        self.files = {"presented.ppm": ppm, "capture.env": capture,
                      "presented.bgra": bgra}
        for name, raw in self.files.items():
            (self.root / name).write_bytes(raw)
        self.frame = self.root / "presented.ppm"

    def test_valid_frame_hashes_exact_parsed_bytes(self):
        images, captures = scanout_hashes([self.frame])
        self.assertEqual(images, [hashlib.sha256(self.files["presented.ppm"]).hexdigest()])
        self.assertEqual(captures, [hashlib.sha256(self.files["capture.env"]).hexdigest()])

    def test_invalid_to_valid_capture_same_length_replacement_refuses(self):
        path = self.root / "capture.env"
        valid = self.files["capture.env"]
        invalid = valid.replace(b"active-cgl-iosurface", b"active-cgl-iosurfacf")
        self.assertEqual(len(invalid), len(valid))
        path.write_bytes(invalid)
        with self.assertRaisesRegex(ValueError, "authenticate"):
            scanout_hashes([self.frame])
        replacement = self.root / "capture-replacement.env"
        replacement.write_bytes(valid)
        self.assert_swap_refused(path, replacement)

    def test_same_length_ppm_and_bgra_replacements_refuse(self):
        for name in ("presented.ppm", "presented.bgra"):
            with self.subTest(name=name):
                path = self.root / name
                altered = bytearray(self.files[name])
                altered[-1] ^= 1
                replacement = self.root / (name + ".replacement")
                replacement.write_bytes(altered)
                self.assertEqual(replacement.stat().st_size, path.stat().st_size)
                self.assert_swap_refused(path, replacement)
                path.write_bytes(self.files[name])

    def assert_swap_refused(self, path: Path, replacement: Path):
        original_open = os.open

        def swap_after_open(candidate, flags, *args):
            fd = original_open(candidate, flags, *args)
            if Path(candidate) == path:
                os.replace(replacement, path)
            return fd

        with patch.object(integrity.os, "open", swap_after_open):
            with self.assertRaisesRegex(ValueError, "changed while reading"):
                scanout_hashes([self.frame])

    def test_fifo_each_input_refuses_without_writer(self):
        for name in self.files:
            with self.subTest(name=name):
                path = self.root / name
                path.unlink()
                os.mkfifo(path)
                result = subprocess.run([sys.executable, "-c", FIFO_CHILD,
                                         str(MODULES), str(self.root)],
                                        stdin=subprocess.DEVNULL, capture_output=True,
                                        text=True, timeout=2)
                self.assertEqual(result.returncode, 0, result.stderr)
                path.unlink()
                path.write_bytes(self.files[name])

    def test_wrong_pixels_and_capture_metadata_refuse(self):
        bgra = self.root / "presented.bgra"
        altered = bytearray(self.files["presented.bgra"])
        altered[0] ^= 1
        bgra.write_bytes(altered)
        with self.assertRaisesRegex(ValueError, "BGRA frame differs"):
            scanout_hashes([self.frame])
        bgra.write_bytes(self.files["presented.bgra"])
        capture = self.root / "capture.env"
        capture.write_bytes(self.files["capture.env"].replace(
            b"active-cgl-iosurface", b"active-cgl-iosurfacf"))
        with self.assertRaisesRegex(ValueError, "authenticate"):
            scanout_hashes([self.frame])


if __name__ == "__main__":
    unittest.main()
