#!/usr/bin/env python3
"""Fake serial peer for unchanged production Swift driver; not a Windows VM."""
import base64
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest

BINARY = sys.argv.pop(1)


class DriverIntegration(unittest.TestCase):
    def exercise(self, mode):
        with tempfile.TemporaryDirectory() as tmp:
            control, log = Path(tmp) / "agent.ctl", Path(tmp) / "run.log"
            control.touch()
            log.write_text("BVAGENT SERVICE start t=1\n")
            process = subprocess.Popen([BINARY, str(control), str(log), "123", "32767"],
                                       stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
            handled, payloads = 0, []
            try:
                deadline = time.monotonic() + 40
                while process.poll() is None and time.monotonic() < deadline:
                    commands = control.read_text().splitlines()
                    for command in commands[handled:]:
                        handled += 1
                        parts = command.split()
                        label = " ".join(parts[:2])
                        if parts[0] == "INPUTCAPS":
                            marker = "BVINPUT_CAPS " + parts[1] + " 3 TEXTINPUT KEYINPUT POINTERINPUT 65536"
                        else:
                            payloads.append((parts[0], base64.b64decode(parts[2]).decode()))
                            counts = [16, 2, 8, 2]
                            count = counts[len(payloads) - 1]
                            if mode == "bad-count":
                                count += 1
                            marker = "BVINPUT_INSERTED " + parts[1] + " " + str(count)
                        response = "BVAGENT CMD " + label + " exit=0\n" + marker + "\r\nBVAGENT END " + label + "\n"
                        if mode == "restart" and payloads:
                            response += "BVAGENT SERVICE start t=2\n"
                        # Exercise incremental reads, including a split receipt header.
                        with log.open("a") as stream:
                            stream.write(response[:7]); stream.flush()
                            time.sleep(0.02)
                            stream.write(response[7:])
                    time.sleep(0.005)
                output, error = process.communicate(timeout=2)
                self.assertEqual(error, "")
                return process.returncode, json.loads(output), payloads
            finally:
                if process.poll() is None:
                    process.kill()
                process.wait(timeout=5)

    def test_real_driver_schedules_burst(self):
        status, report, payloads = self.exercise("success")
        self.assertEqual(status, 0)
        self.assertTrue(report["driver_receipts_observed"])
        self.assertEqual(report["sent"], 4)
        self.assertEqual(report["inserted"], 4)
        self.assertFalse(report["production_ui_proven"])
        self.assertFalse(report["guest_application_proven"])
        self.assertEqual(payloads, [("TEXTINPUT", "BridgeVM"), ("KEYINPUT", "enter"),
                                    ("TEXTINPUT", "\uD55C\uAE00\U0001F642"), ("POINTERINPUT", "click:123x32767")])

    def test_rejected_receipt_never_advances(self):
        for mode in ("bad-count", "restart"):
            with self.subTest(mode=mode):
                status, report, payloads = self.exercise(mode)
                self.assertNotEqual(status, 0)
                self.assertFalse(report["driver_receipts_observed"])
                self.assertEqual(len(payloads), 1)


if __name__ == "__main__":
    unittest.main()
