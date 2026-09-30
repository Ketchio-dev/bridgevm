#!/usr/bin/env python3
"""T17 sends pointer and keyboard input only after the guest challenge form reports it is shown."""
from __future__ import annotations

import re
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
E2E = ROOT / "apps/macos/Sources/BridgeVMProductE2E"
GUEST = ROOT / "scripts/win-assets/bv-product-e2e.ps1"
READY_WRITE = 'Write-Exact "t17-keyboard-pointer-ready-$Prefix.txt" "bridgevm-t17-keyboard-pointer-ready-v1`n$Nonce`n"'
INPUTS = ('ui.press("bridgevm.runtime.display.open"', "ui.clickSecondaryWindow(",
          'ui.fill("t17kbd\\(prefix)", identifier: "bridgevm.runtime.keyboard.input"',
          'ui.press("bridgevm.runtime.keyboard.send"')


def body(source: str, signature: str) -> str:
    return source.split(signature, 1)[1].split("\n    }\n", 1)[0]


class InputChallengeReady(unittest.TestCase):
    def test_guest_reports_ready_only_from_the_shown_form(self) -> None:
        raw = GUEST.read_bytes()
        self.assertEqual(raw.count(b"\n"), raw.count(b"\r\n"))
        source = raw.decode("utf-8").replace("\r\n", "\n")
        challenge = source.split("'KeyboardPointer' {", 1)[1].split("'Clipboard' {", 1)[0]
        shown = challenge.split("$Form.Add_Shown({", 1)[1].split("})", 1)[0]
        self.assertEqual(source.count('"t17-keyboard-pointer-ready-'), 1)
        order = [shown.index(step) for step in ("$Form.Activate()", "$Form.Focus()", "$Timer.Start()", READY_WRITE)]
        self.assertEqual(order, sorted(order))
        self.assertLess(challenge.index("$Form.Add_Shown({"), challenge.index("$Form.ShowDialog()"))

    def test_host_waits_for_the_exact_marker_before_any_input(self) -> None:
        journey = body((E2E / "T17GuestJourney.swift").read_text(encoding="utf-8"), "private func keyboardAndPointer() throws {")
        steps = ['try launchWorkload("KeyboardPointer")',
                 "try T17InputChallenge(sharePath: request.sharePath, nonce: request.nonce, ui: ui).deliver()",
                 'try requireOutput("t17-keyboard-pointer-\\(prefix).txt", timeout: 60)']
        self.assertEqual([line.strip() for line in journey.strip().splitlines()], steps)
        source = (E2E / "T17InputChallenge.swift").read_text(encoding="utf-8")
        deliver = body(source, "func deliver() throws {")
        order = [deliver.index("try waitUntilShown()"), *(deliver.index(call) for call in INPUTS)]
        self.assertEqual(order, sorted(order))
        wait = body(source, "private func waitUntilShown() throws {")
        self.assertIn('appendingPathComponent("t17-keyboard-pointer-ready-\\(prefix).txt")', wait)
        self.assertIn('Data("bridgevm-t17-keyboard-pointer-ready-v1\\n\\(nonce)\\n".utf8)', wait)
        self.assertIn('"guest input challenge was not shown"', wait)
        self.assertEqual(re.findall(r"var readyTimeout: TimeInterval = (\d+)", source), ["60"])

    def test_no_other_product_path_sends_challenge_input(self) -> None:
        for path in E2E.glob("*.swift"):
            if path.name == "T17InputChallenge.swift":
                continue
            text = path.read_text(encoding="utf-8")
            for call in INPUTS:
                self.assertFalse(call in text, f"{path.name}: {call}")


if __name__ == "__main__":
    result = unittest.main(argv=[sys.argv[0]], exit=False).result
    raise SystemExit(0 if result.wasSuccessful() else 1)
