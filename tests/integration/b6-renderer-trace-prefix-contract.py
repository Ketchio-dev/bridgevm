"""Regression contracts for the renderer's observed diagnostic prefixes."""
import hashlib
import pathlib
import sys
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/live-gates"))
import b6_renderer_trace as evidence

TGSI = b"venus-win32: TGSI received:venus-win32: "
GLSL = b"venus-win32: GLSL:venus-win32: "


class PrefixContracts(unittest.TestCase):
    def check_log(self, raw, expected):
        with tempfile.TemporaryDirectory() as directory:
            path = pathlib.Path(directory) / "renderer.log"
            path.write_bytes(raw)
            result = evidence.log_evidence(path)
        self.assertEqual((result["tgsi_headers"], result["glsl_headers"]), expected)
        self.assertEqual(result["sha256"], hashlib.sha256(raw).hexdigest())
        self.assertEqual(result["bytes"], len(raw))

    def test_observed_prefixes_and_line_endings(self):
        for stage in (b"VERT", b"FRAG", b"GEOM", b"TESS_CTRL", b"TESS_EVAL", b"COMP"):
            for newline in (b"\n", b"\r\n"):
                with self.subTest(stage=stage, newline=newline):
                    self.check_log(TGSI + stage + newline + GLSL + b"#version 140" + newline, (1, 1))

    def test_other_prefixes_and_mixed_categories_do_not_count(self):
        for raw in (
            b"notice: " + TGSI + b"FRAG\nnotice: " + GLSL + b"#version 140\n",
            b"other: TGSI received:other: FRAG\nother: GLSL:other: #version 140\n",
            GLSL + b"FRAG\n" + TGSI + b"#version 140\n",
            TGSI + b"FRAG suffix\n" + GLSL + b"not #version 140\n",
            b"VREND_DEBUG=shader,cmd,obj,d3d\n#extension GL_ARB_shader_bit_encoding : require\n",
        ):
            with self.subTest(raw=raw):
                self.check_log(raw, (0, 0))

    def test_bare_headers_remain_supported_and_raw_bytes_are_not_rewritten(self):
        raw = b"VERT\n#version 150\n" + TGSI + b"FRAG\r\n" + GLSL + b"#version 140\r\n\xff\n"
        self.check_log(raw, (2, 2))


if __name__ == "__main__":
    unittest.main()
