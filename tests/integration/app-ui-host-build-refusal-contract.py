#!/usr/bin/env python3
"""Pure refusal-parser contracts; no compiler, app, AX, or permission query."""
import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("refusal", ROOT / "scripts/app_ui_host_build_refusal.py")
refusal = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(refusal)
SOURCE = ROOT / "apps/macos/Sources/BridgeVMControl/BridgeVMControlMain.swift"
MARKER = b"BRIDGEVM_APP_UI_HOST requires a separate DEBUG diagnostic build"


class RefusalContracts(unittest.TestCase):
    def setUp(self):
        self.path = str(SOURCE)
        self.source = SOURCE.read_bytes()
        self.location = self.path.encode() + b":5:8"
        # Layouts observed from actual Swift 6.4 native and swiftbuild refusals.
        self.native = self.location + b": error: " + MARKER
        self.swiftbuild = b"error: " + self.location + b" " + MARKER

    def accepts(self, log, *, status="1", path=None, source=None):
        return refusal.matches_refusal(log, self.path if path is None else path,
                                       status, self.source if source is None else source)

    def test_both_observed_formats_with_or_without_terminal_newline(self):
        for line in [self.native, self.swiftbuild]:
            for log in [line, line + b"\n", b"Building...\n" + line + b"\nBuild failed\n"]:
                with self.subTest(log=log):
                    self.assertTrue(self.accepts(log))

    def test_nonzero_compiler_status_is_required(self):
        for status in ["1", "127", "255"]:
            self.assertTrue(self.accepts(self.native, status=status))
        for status in ["", "0", "-1", "256", "01", "+1", "1.0", "1\n", " 1", "١"]:
            with self.subTest(status=status):
                self.assertFalse(self.accepts(self.swiftbuild, status=status))

    def test_warning_note_source_echo_and_embedded_diagnostic_are_rejected(self):
        for line in [self.native, self.swiftbuild]:
            for invalid in [line.replace(b"error:", b"warning:"), line.replace(b"error:", b"note:"),
                            b"note: " + line, b"5 | " + line, b'print("' + line + b'")',
                            b"prefix " + line, line + b" suffix", line + b"\r", line + b"\0",
                            b"\x1b[31m" + line + b"\x1b[0m"]:
                with self.subTest(invalid=invalid):
                    self.assertFalse(self.accepts(invalid))
        for echo in [b'#error("' + MARKER + b'")', b'5 | #error("' + MARKER + b'")',
                     b"  |        `- error: " + MARKER]:
            self.assertFalse(self.accepts(echo))

    def test_exact_caller_source_location_and_marker_are_required(self):
        for line in [self.native, self.swiftbuild]:
            for invalid in [line.replace(self.path.encode(), b"/other/BridgeVMControlMain.swift"),
                            line.replace(b"BridgeVMControlMain.swift", b"OtherBridgeVMControlMain.swift"),
                            line.replace(b":5:8", b":6:8"), line.replace(b":5:8", b":5:9"),
                            line.replace(b":5:8", b":05:08"), line.replace(MARKER, MARKER[:-1]),
                            line.replace(MARKER, MARKER.lower()), line.replace(b"error:", b"error :")]:
                with self.subTest(invalid=invalid):
                    self.assertFalse(self.accepts(invalid))

    def test_split_or_indented_diagnostics_do_not_form_a_match(self):
        for line in [self.native, self.swiftbuild]:
            for invalid in [line.replace(MARKER, b"\n" + MARKER),
                            line.replace(b":5:8", b":5:\n8"), b" " + line, b"\t" + line,
                            b"prefix\xe2\x80\xa8" + line, line + b"\xe2\x80\xa8suffix"]:
                with self.subTest(invalid=invalid):
                    self.assertFalse(self.accepts(invalid))

    def test_source_guard_must_still_be_at_the_expected_location(self):
        for source in [b"", b"\n" + self.source,
                       self.source.replace(b"#if BRIDGEVM_APP_UI_HOST && !DEBUG", b"#if true"),
                       self.source.replace(b'#error("' + MARKER + b'")', b'#error("' + MARKER + b'") '),
                       self.source.replace(b'#error("' + MARKER + b'")', b'print("' + MARKER + b'")'),
                       self.source.replace(b"#endif", b"// #endif", 1)]:
            self.assertFalse(self.accepts(self.native, source=source))

    def test_absolute_source_path_is_literal(self):
        for path in ["BridgeVMControlMain.swift", "/fixture/Other.swift", "/fixture\n/BridgeVMControlMain.swift"]:
            line = path.encode() + b":5:8: error: " + MARKER
            self.assertFalse(self.accepts(line, path=path))
        path = "/fixture with spaces/BridgeVMControlMain.swift"
        self.assertTrue(self.accepts(path.encode() + b":5:8: error: " + MARKER, path=path))


if __name__ == "__main__":
    unittest.main()
