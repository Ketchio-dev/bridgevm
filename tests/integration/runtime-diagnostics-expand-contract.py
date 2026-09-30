#!/usr/bin/env python3
"""T17 opens collapsed runtime diagnostics through the AXDisclosureTriangle SwiftUI exposes."""
from __future__ import annotations

import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
PRODUCT_E2E = ROOT / "apps/macos/Sources/BridgeVMProductE2E"


class ExpandContract(unittest.TestCase):
    def test_opener_is_expanded_not_pressed_as_a_button(self) -> None:
        entry = (PRODUCT_E2E / "T17RuntimeControlInput.swift").read_text(encoding="utf-8")
        self.assertIn("ui.expand(opener", entry)
        self.assertNotIn("ui.press(opener", entry)

    def test_expand_targets_the_disclosure_triangle_role(self) -> None:
        accessibility = (PRODUCT_E2E / "T17Accessibility.swift").read_text(encoding="utf-8")
        expand = accessibility.split("func expand(", 1)[1].split("\n    }", 1)[0]
        self.assertIn("kAXDisclosureTriangleRole", expand)
        self.assertIn("func expand(_ identifier: String, timeout: TimeInterval) throws",
                      (PRODUCT_E2E / "T17UIControlling.swift").read_text(encoding="utf-8"))


if __name__ == "__main__":
    result = unittest.main(argv=[sys.argv[0]], exit=False).result
    raise SystemExit(0 if result.wasSuccessful() else 1)
