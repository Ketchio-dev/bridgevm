#!/usr/bin/env python3
"""PPM boundary and single-read reference identity contracts, without a GPU."""
import hashlib
import importlib.util
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
from glyph_frame import Frame


def module(name, filename):
    spec = importlib.util.spec_from_file_location(name, ROOT / "scripts" / filename)
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


builder = module("glyph_builder_contract", "build-glyph-pixel-mask.py")
verifier = module("glyph_verifier_contract", "verify-glyph-pixel-mask.py")


class PPMContracts(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)

    def frame(self, raw):
        path = self.root / "frame.ppm"
        path.write_bytes(raw)
        return path

    def test_header_variants_preserve_first_raster_bytes(self):
        pixels = bytes([10, 13, 9, 32, 255, 0])
        for separator in (b"\n", b"\r\n", b"\r", b" ", b"\t"):
            with self.subTest(separator=separator):
                raw = b"P6\n2 1\n255" + separator + pixels
                path = self.frame(raw)
                frame = Frame.load(path)
                self.assertEqual((frame.width, frame.height, frame.pixels), (2, 1, pixels))
                self.assertEqual(frame.source_sha256, hashlib.sha256(raw).hexdigest())
                self.assertEqual(builder.load_ppm(path), (2, 1, bytearray(pixels)))

    def test_comments_and_header_whitespace(self):
        pixels = bytes([0, 1, 2])
        frame = Frame.load(self.frame(b"P6\t# comment\n1\v1\f255\n" + pixels))
        self.assertEqual(frame.pixels, pixels)

    def test_truncated_headers_fail_instead_of_looping(self):
        for raw in (b"", b"P6", b"P6\n1", b"P6\n1 1", b"P6\n1 1\n255", b"P6\n# unfinished"):
            with self.subTest(raw=raw):
                path = self.frame(raw)
                with self.assertRaises(ValueError):
                    Frame.load(path)
                with self.assertRaises(ValueError):
                    builder.load_ppm(path)

    def test_exact_raster_length(self):
        for pixels in (b"", b"\x00\x01", b"\x00\x01\x02\x03"):
            with self.subTest(pixels=pixels):
                with self.assertRaises(ValueError):
                    Frame.load(self.frame(b"P6\n1 1\n255\n" + pixels))

    def test_invalid_magic_maximum_and_geometry(self):
        for header in (b"P3\n1 1\n255\n", b"P6\n1 1\n65535\n", b"P6\n0 1\n255\n",
                       b"P6\n1 -1\n255\n", b"P6\nword 1\n255\n"):
            with self.subTest(header=header):
                with self.assertRaises(ValueError):
                    Frame.load(self.frame(header + bytes([0, 1, 2])))

    def changing_reference(self, reference, original, changed):
        reads = []
        read_bytes = Path.read_bytes

        def read(path):
            if path == reference:
                reads.append(path)
                return original if len(reads) == 1 else changed
            return read_bytes(path)
        return reads, patch.object(Path, "read_bytes", read)

    def test_builder_hashes_the_pixels_it_selected(self):
        raw = b"P6\n16 16\n255\n" + bytes([0] * 384 + [255] * 384)
        changed = b"P6\n16 16\n255\n" + bytes([255] * 384 + [0] * 384)
        reference = self.frame(raw)
        reads, replacement = self.changing_reference(reference, raw, changed)
        with replacement:
            mask = builder.build(reference, {"caption": (0, 0, 16, 16)}, self.root / "mask.json", None)
        self.assertEqual(len(reads), 1)
        self.assertEqual(mask["reference_sha256"], hashlib.sha256(raw).hexdigest())

    def test_verifier_compares_the_authenticated_reference_read(self):
        raw = b"P6\n2 1\n255\n" + bytes([0, 0, 0, 255, 255, 255])
        changed = b"P6\n2 1\n255\n" + bytes([255, 255, 255, 0, 0, 0])
        reference = self.frame(raw)
        captures = [self.root / ("capture%d.ppm" % i) for i in range(3)]
        for capture in captures:
            capture.write_bytes(raw)
        mask = {"reference_sha256": hashlib.sha256(raw).hexdigest(),
                "regions": {"caption": {"box": [0, 0, 2, 1], "pixels": [0, 1]}}}
        reads, replacement = self.changing_reference(reference, raw, changed)
        with replacement:
            result = verifier.verify_cell(mask, reference, captures)
        self.assertEqual(len(reads), 1)
        self.assertTrue(result["matches_reference"])

    def test_verifier_keeps_public_frame_alias(self):
        self.assertIs(verifier.Frame, Frame)


if __name__ == "__main__":
    unittest.main()
