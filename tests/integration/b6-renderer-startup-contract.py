#!/usr/bin/env python3
"""Renderer startup errors invalidate even otherwise complete trace collection."""
import pathlib
import sys
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/live-gates"))
from b6_renderer_trace import log_evidence


class StartupContract(unittest.TestCase):
    def observe(self, content):
        with tempfile.TemporaryDirectory(prefix="b6 startup ") as directory:
            path = pathlib.Path(directory) / "run.log"
            path.write_bytes(content)
            return log_evidence(path)

    def test_trace_without_startup_error(self):
        result = self.observe(b"FRAG\n#version 410\n")
        self.assertEqual(result["tgsi_headers"], 1)
        self.assertEqual(result["glsl_headers"], 1)

    def test_exec_failure_before_success_banner_is_rejected(self):
        with self.assertRaisesRegex(ValueError, "renderer startup failed"):
            self.observe(b"proxy: failed to exec /missing/server: No such file or directory\n"
                         b"virtio-gpu: venus 3D backend enabled mode=threaded\nFRAG\n#version 410\n")

    def test_exec_failure_after_trace_is_rejected(self):
        with self.assertRaisesRegex(ValueError, "renderer startup failed"):
            self.observe(b"FRAG\n#version 410\nproxy: failed to exec /missing/server: denied\n")

    def test_venus_init_failure_is_rejected(self):
        with self.assertRaisesRegex(ValueError, "renderer startup failed"):
            self.observe(b"failed to initialize venus renderer\n")

    def test_crlf_exec_failure_is_rejected(self):
        with self.assertRaisesRegex(ValueError, "renderer startup failed"):
            self.observe(b"proxy: failed to exec /missing/server: absent\r\n")

    def test_missing_newline_exec_failure_is_rejected(self):
        with self.assertRaisesRegex(ValueError, "renderer startup failed"):
            self.observe(b"proxy: failed to exec /missing/server: absent")

    def test_nonfatal_gl_warning_is_not_reclassified(self):
        result = self.observe(b"Running without ARB/KHR robustness in place may crash\n"
                              b"FRAG\n#version 410\n")
        self.assertEqual(result["glsl_headers"], 1)


if __name__ == "__main__":
    unittest.main()
