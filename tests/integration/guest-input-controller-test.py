#!/usr/bin/env python3
"""Synthetic fail-closed contracts; no Windows application is driven."""
import base64
import importlib.util
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("controller", ROOT / "scripts/live-gates/guest_input_controller.py")
c = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(c)


class Contracts(unittest.TestCase):
    def test_complete_envelope(self):
        label, receipt = "TEXTINPUT nonce", "BVINPUT_INSERTED nonce 16"
        good = ["BVAGENT CMD " + label + " exit=0", receipt, "BVAGENT END " + label]
        self.assertTrue(c.completed_reply(good, label, receipt))
        self.assertFalse(c.completed_reply(good[:2], label, receipt))
        self.assertFalse(c.completed_reply([receipt], label, receipt))
        for bad in (good + good, [good[0].replace("=0", "=1"), *good[1:]],
                    [good[0], receipt.replace("16", "15"), good[2]],
                    [good[0], receipt, receipt, good[2]], [good[2], *good[:2]]):
            with self.assertRaises(ValueError):
                c.completed_reply(bad, label, receipt)

    def test_ready(self):
        ready = dict(schema="bridgevm.input-sink-ready.v1", nonce="fresh", foreground=True,
                     first_text=c.FIRST, second_text_base64=base64.b64encode(c.SECOND.encode()).decode(),
                     button_x=0, button_y=32767)
        self.assertEqual(c.ready_coordinates(ready, "fresh"), (0, 32767))
        for field, bad in (("nonce", "stale"), ("foreground", 1), ("button_x", True),
                           ("button_y", 32768), ("button_x", -1), ("button_x", 1.0),
                           ("second_text_base64", "")):
            with self.assertRaises(ValueError):
                c.ready_coordinates(dict(ready, **{field: bad}), "fresh")

    def test_result(self):
        result = dict(schema="bridgevm.input-sink.v1", nonce="fresh", passed=True,
                      clicked=True, text_matches=True, enter_count=1, enter_saw_first_text=True,
                      focus_lost=False, actual_text_sha256=c.EXPECTED_HASH, reason="clicked")
        c.check_result(result, "fresh")
        for field, bad in (("nonce", "stale"), ("passed", 1), ("clicked", False),
                           ("text_matches", False), ("enter_count", True), ("enter_count", 2),
                           ("enter_saw_first_text", False), ("focus_lost", True),
                           ("actual_text_sha256", "0" * 64), ("reason", "timeout")):
            with self.assertRaises(ValueError):
                c.check_result(dict(result, **{field: bad}), "fresh")

    def test_file_limits_and_symlinks(self):
        with tempfile.TemporaryDirectory() as tmp:
            file = Path(tmp) / "file"
            file.write_bytes(b"123")
            self.assertEqual(c.regular_bytes(file, 3), b"123")
            with self.assertRaises(ValueError):
                c.regular_bytes(file, 2)
            link = Path(tmp) / "link"
            link.symlink_to(file)
            with self.assertRaises(OSError):
                c.regular_bytes(link, 3)

    def test_input_wire(self):
        driver = c.Controller("unused", "unused", "unused")
        calls = []
        driver.send = lambda *args: calls.append(args)
        driver.input("KEYINPUT", "enter", 2)
        wire, label, receipt = calls[0]
        self.assertEqual(base64.b64decode(wire.split()[2]), b"enter")
        self.assertEqual(receipt, "BVINPUT_INSERTED " + label.split()[1] + " 2")
        self.assertEqual(len(c.SECOND.encode("utf-16-le")), 8)


if __name__ == "__main__":
    unittest.main()
