#!/usr/bin/env python3
"""T17 sends pointer and keyboard input only after the guest challenge form reports it can take input."""
from __future__ import annotations

import re
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
E2E = ROOT / "apps/macos/Sources/BridgeVMProductE2E"
GUEST = ROOT / "scripts/win-assets/bv-product-e2e.ps1"
READY_WRITE = 'Write-Exact "t17-keyboard-pointer-ready-$Prefix.txt" "bridgevm-t17-keyboard-pointer-ready-v1`n$Nonce`n"'
INPUTS = ('ui.press("bridgevm.runtime.display.open"', "ui.clickDisplaySurface(",
          'ui.fill("t17kbd\\(prefix)", identifier: "bridgevm.runtime.keyboard.input"',
          'ui.press("bridgevm.runtime.keyboard.send"')


def body(source: str, signature: str) -> str:
    return source.split(signature, 1)[1].split("\n    }\n", 1)[0]


class InputChallengeReady(unittest.TestCase):
    def test_guest_reports_ready_only_once_a_user_could_act(self) -> None:
        source = GUEST.read_bytes().decode("utf-8").replace("\r\n", "\n")
        challenge = source.split("'KeyboardPointer' {", 1)[1].split("'Clipboard' {", 1)[0]
        ready = challenge.split("$ReadyTimer.Add_Tick({", 1)[1].split("})", 1)[0]
        self.assertEqual(source.count('"t17-keyboard-pointer-ready-'), 1)
        self.assertLess(ready.index("[BridgeVM.InputDesktop]::Accepts($Form.Handle)"), ready.index(READY_WRITE))
        shown = challenge.split("$Form.Add_Shown({", 1)[1].split("})", 1)[0]
        order = [shown.index(step) for step in ("$Form.Activate()", "$Timer.Start()", "$ReadyTimer.Start()")]
        self.assertEqual(order, sorted(order))

    def test_host_waits_for_the_exact_marker_before_any_input(self) -> None:
        journey = body((E2E / "T17GuestJourney.swift").read_text(encoding="utf-8"), "private func keyboardAndPointer() throws {")
        steps = ['try launchWorkload("KeyboardPointer")',
                 "try T17InputChallenge(sharePath: request.sharePath, nonce: request.nonce, ui: ui, runLog: runLog).deliver()"]
        self.assertEqual([line.strip() for line in journey.strip().splitlines()], steps)
        source = (E2E / "T17InputChallenge.swift").read_text(encoding="utf-8")
        deliver = body(source, "func deliver() throws {")
        order = [deliver.index("T17InputChallengeShare.waitUntilShown("), *(deliver.index(call) for call in INPUTS),
                 deliver.index("T17InputChallengeShare.awaitOutput(")]
        self.assertEqual(order, sorted(order))
        wait = body((E2E / "T17InputChallengeShare.swift").read_text(encoding="utf-8"), "static func waitUntilShown(")
        self.assertIn('appendingPathComponent("t17-keyboard-pointer-ready-\\(prefix(nonce)).txt")', wait)
        self.assertIn('Data("bridgevm-t17-keyboard-pointer-ready-v1\\n\\(nonce)\\n".utf8)', wait)
        self.assertIn('"guest input challenge was not shown"', wait)
        self.assertEqual(re.findall(r"var readyTimeout: TimeInterval = (\d+)", source), ["180"])

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
