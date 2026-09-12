#!/usr/bin/env python3
"""Protocol framing regressions, deliberately not native input evidence."""
import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("controller", ROOT / "scripts/live-gates/guest_input_controller.py")
c = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(c)


class Framing(unittest.TestCase):
    def test_crlf_envelope(self):
        wire = "BVAGENT CMD label exit=0\nBVINPUT_INSERTED fresh 2\r\n\nBVAGENT END label\n"
        lines = wire.replace("\r", "\n").splitlines()
        self.assertEqual(c.completed_reply(lines, "label", "BVINPUT_INSERTED fresh 2"),
                         "BVINPUT_INSERTED fresh 2")
        lines.insert(2, "BVINPUT_FAILED fresh partial")
        with self.assertRaises(ValueError):
            c.completed_reply(lines, "label", "BVINPUT_INSERTED fresh 2")

    def test_pending_then_ready_is_read_only(self):
        driver = c.Controller("unused", "unused", "unused")
        calls = []
        def send(command, label, choices):
            self.assertEqual(command, label)
            self.assertIn("Get-FileHash -LiteralPath", command)
            self.assertNotIn("Invoke-CimMethod", command)
            calls.append(command)
            return choices[1] if len(calls) == 1 else choices[0]
        driver.send = send
        driver.await_staged(r"C:\BridgeVM\input-proof\bv-input-order-sink.ps1", "A" * 64)
        self.assertEqual(len(calls), 2)
        self.assertNotEqual(calls[0], calls[1])

    def test_fixed_order_and_pointer_contract(self):
        rows = c.input_sequence(123, 32767)
        self.assertEqual([r[0] for r in rows], ["TEXTINPUT", "KEYINPUT", "TEXTINPUT", "POINTERINPUT"])
        self.assertEqual([r[2] for r in rows], [16, 2, 8, 2])
        self.assertEqual(rows[-1][1], "click:123x32767")
        for x, y in ((True, 0), (-1, 0), (0, 32768)):
            with self.assertRaises(ValueError):
                c.input_sequence(x, y)

    def test_pending_requires_complete_success(self):
        choices = ("BVINPUT_STAGED fresh", "BVINPUT_PENDING fresh")
        lines = ["BVAGENT CMD label exit=0", choices[1], "BVAGENT END label"]
        self.assertEqual(c.completed_reply(lines, "label", choices), choices[1])
        self.assertFalse(c.completed_reply(lines[:2], "label", choices))
        lines[0] = "BVAGENT CMD label exit=1"
        with self.assertRaises(ValueError):
            c.completed_reply(lines, "label", choices)


if __name__ == "__main__":
    unittest.main()
