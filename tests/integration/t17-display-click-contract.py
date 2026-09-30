#!/usr/bin/env python3
"""T17 clicks the guest display only once it shows a frame, and proves the click reached the guest."""
from __future__ import annotations

import re
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
E2E = ROOT / "apps/macos/Sources/BridgeVMProductE2E"
ENGINE = ROOT / "apps/macos/Sources/BridgeVMControl/HvfEngine"


def text(path: Path) -> str:
    return path.read_text(encoding="utf-8")


class DisplayClick(unittest.TestCase):
    def test_helper_and_product_name_the_same_surface(self) -> None:
        product = re.findall(r'static let identifier = "([^"]+)"', text(ENGINE / "HvfDisplaySurfaceAccessibility.swift"))
        helper = re.findall(r'static let surface = "([^"]+)"', text(E2E / "T17DisplayClick.swift"))
        self.assertEqual(product, ["bridgevm.runtime.display.surface"])
        self.assertEqual(helper, product)

    def test_surface_reports_the_frame_that_gates_pointer_input(self) -> None:
        view = text(ENGINE / "HvfFramebufferView.swift")
        self.assertIn("HvfDisplaySurfaceAccessibility.configure(self)", view.split("init(session: HvfEngineSession) {", 1)[1].split("\n    }\n", 1)[0])
        self.assertIn("didSet { if guestSize != oldValue { HvfDisplaySurfaceAccessibility.update(self, guestSize: guestSize) } }", view)
        down = view.split("override func mouseDown(with event: NSEvent) {", 1)[1].split("\n    }\n", 1)[0]
        self.assertIn("guard let session, hasGuestSize else {", down)
        self.assertIn("guestSize.width > 0 && guestSize.height > 0", view)
        self.assertIn('guard size.width > 0, size.height > 0 else { return "no-frame" }', text(ENGINE / "HvfDisplaySurfaceAccessibility.swift"))
        self.assertIn('target.value?.hasPrefix("frame ") == true, target.focused', text(E2E / "T17DisplayClick.swift"))

    def test_keyboard_waits_for_inserted_pointer_receipts(self) -> None:
        deliver = text(E2E / "T17InputChallenge.swift").split("func deliver() throws {", 1)[1].split("\n    }\n", 1)[0]
        click = "try T17PointerReceipt.require(runLog, timeout: 15) { try ui.clickDisplaySurface(timeout: 15) }"
        self.assertEqual(deliver.count(click), 1)
        self.assertLess(deliver.index("bridgevm.runtime.display.open"), deliver.index(click))
        self.assertLess(deliver.index(click), deliver.index("bridgevm.runtime.keyboard.input"))
        receipt = text(E2E / "T17PointerReceipt.swift")
        self.assertIn("static let clickInsertions = 2", receipt)
        self.assertIn('fields[2] == "POINTERINPUT", fields[4] == "exit=0"', receipt)

    def test_only_the_display_click_posts_mouse_events(self) -> None:
        for path in E2E.glob("*.swift"):
            if path.name != "T17DisplayClick.swift":
                self.assertNotIn("mouseEventSource", text(path), path.name)


if __name__ == "__main__":
    result = unittest.main(argv=[sys.argv[0]], exit=False).result
    raise SystemExit(0 if result.wasSuccessful() else 1)
