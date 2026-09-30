#!/usr/bin/env python3
"""T17 fills send-button fields with focus and no confirm, as SwiftUI needs for their binding."""
from __future__ import annotations

import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CONTROL = ROOT / "apps/macos/Sources/BridgeVMControl/HvfEngine"
E2E = ROOT / "apps/macos/Sources/BridgeVMProductE2E"
FIELDS = {
    "bridgevm.runtime.ctl.input": ("HvfRuntimeDiagnosticsSettings.swift", "onCommit: sendCtl", "T17RuntimeControlInput.swift", "ui.fill(command, identifier: input"),
    "bridgevm.runtime.keyboard.input": ("HvfRuntimeKeyboardInput.swift", "onCommit: submit", "T17InputChallenge.swift", 'ui.fill("t17kbd\\(prefix)", identifier: "bridgevm.runtime.keyboard.input"'),
}


class SendFieldEntry(unittest.TestCase):
    def test_send_fields_commit_through_their_own_action(self) -> None:
        for identifier, (view, commit, _, _) in FIELDS.items():
            source = (CONTROL / view).read_text(encoding="utf-8")
            self.assertIn(commit, source, identifier)
            self.assertIn(f'"{identifier}"', source, identifier)

    def test_t17_fills_them_instead_of_confirming(self) -> None:
        for identifier, (_, _, harness, call) in FIELDS.items():
            source = (E2E / harness).read_text(encoding="utf-8")
            self.assertIn(call, source, identifier)
            self.assertNotIn(f'ui.setText("t17kbd', source)
            self.assertNotIn("ui.setText(command", source)
        entry = (E2E / "T17TextEntry.swift").read_text(encoding="utf-8")
        fill = entry.split("static func fill(", 1)[1]
        self.assertLess(fill.index("focus()"), fill.index("set(value)"))
        self.assertNotIn("confirm", fill.split("\n    }", 1)[0])


if __name__ == "__main__":
    result = unittest.main(argv=[sys.argv[0]], exit=False).result
    raise SystemExit(0 if result.wasSuccessful() else 1)
