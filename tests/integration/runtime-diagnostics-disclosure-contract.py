#!/usr/bin/env python3
"""T17 reaches identifiers inside the collapsed runtime diagnostics group only through its opener."""
from __future__ import annotations

import re
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SETTINGS = ROOT / "apps/macos/Sources/BridgeVMControl/HvfEngine/HvfRuntimeDiagnosticsSettings.swift"
TOGGLE = ROOT / "apps/macos/Sources/BridgeVMControl/HvfEngine/HvfRuntimeDiagnosticsToggle.swift"
PRODUCT_E2E = ROOT / "apps/macos/Sources/BridgeVMProductE2E"
OPENER = "bridgevm.runtime.diagnostics.toggle"
ENTRY = "T17RuntimeControlInput.swift"


def collapsed_identifiers() -> set[str]:
    source = SETTINGS.read_text(encoding="utf-8")
    body = source.split("DisclosureGroup(", 1)[1].split("} label:", 1)[0]
    return set(re.findall(r'accessibilityIdentifier\("([^"]+)"\)', body))


class DisclosureContract(unittest.TestCase):
    def test_collapsed_group_has_a_pressable_opener(self) -> None:
        self.assertIn("isExpanded: $expanded", SETTINGS.read_text(encoding="utf-8"))
        self.assertIn(f'accessibilityIdentifier("{OPENER}")', TOGGLE.read_text(encoding="utf-8"))
        self.assertIn("HvfRuntimeDiagnosticsToggle(expanded: $expanded)", SETTINGS.read_text(encoding="utf-8"))

    def test_t17_uses_collapsed_identifiers_only_through_the_opener(self) -> None:
        hidden = collapsed_identifiers()
        self.assertIn("bridgevm.runtime.ctl.input", hidden)
        for path in sorted(PRODUCT_E2E.glob("*.swift")):
            text = path.read_text(encoding="utf-8")
            used = sorted(identifier for identifier in hidden if f'"{identifier}"' in text)
            if path.name == ENTRY:
                self.assertIn(f'"{OPENER}"', text)
                continue
            self.assertEqual(used, [], f"{path.name} reaches collapsed diagnostics without opening them")


if __name__ == "__main__":
    result = unittest.main(argv=[sys.argv[0]], exit=False).result
    raise SystemExit(0 if result.wasSuccessful() else 1)
