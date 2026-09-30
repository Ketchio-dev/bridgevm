#!/usr/bin/env python3
"""The T17 challenge form is ready only on the user's input desktop, under the click point, and says what it saw."""
from __future__ import annotations

import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
GUEST = ROOT / "scripts/win-assets/bv-product-e2e.ps1"
SHARE = ROOT / "apps/macos/Sources/BridgeVMProductE2E/T17InputChallengeShare.swift"
WINDOWS = ROOT / ".github/workflows/t17-guest-challenge-contract.yml"


def challenge() -> str:
    source = GUEST.read_bytes().decode("utf-8").replace("\r\n", "\n")
    return source.split("'KeyboardPointer' {", 1)[1].split("'Clipboard' {", 1)[0]


class InputChallengeDesktop(unittest.TestCase):
    def test_ready_needs_the_default_input_desktop_and_the_form_under_the_screen_centre(self) -> None:
        accepts = challenge().split("public static bool Accepts(IntPtr form) {", 1)[1].split("\n}\n", 1)[0]
        for check in ("OpenInputDesktop(0, false, 1); if (desktop == IntPtr.Zero) { return false; }",
                      'name.ToString() == "Default"', "center.X = GetSystemMetrics(0) / 2; center.Y = GetSystemMetrics(1) / 2;",
                      "return active && GetAncestor(WindowFromPoint(center), 2) == form;"):
            self.assertIn(check, accepts)

    def test_form_stays_on_top_and_reports_progress(self) -> None:
        body, raw = challenge(), GUEST.read_bytes()
        self.assertEqual(raw.count(b"\n"), raw.count(b"\r\n"))
        for needle in ("TopMost = $true", 'Write-Exact "t17-keyboard-pointer-progress-$Prefix.txt" $State', " dismissed=$([Math]::Min($script:Dismissed, 99))`n",
                       '"clicked=$([int]$script:Clicked) typed=$([Math]::Min($script:Typed.Length, 999)) session=$Session integrity=$Integrity foreground=$Foreground cursor=',
                       "if ([BridgeVM.InputDesktop]::ShellFlyout()) { $script:Dismissed++; [Windows.Forms.SendKeys]::SendWait('{ESC}') }"):
            self.assertIn(needle, body)
        self.assertLess(body.index("$Form.Add_Shown({"), body.index("$Form.ShowDialog()"))

    def test_host_reports_only_an_exact_progress_line(self) -> None:
        share = SHARE.read_text(encoding="utf-8")
        for needle in ('cursor=[0-9]{1,4}x[0-9]{1,4} dismissed=[0-9]{1,2}$"#', "data.count <= 128", '" (guest form saw \\($0))"'):
            self.assertIn(needle, share)

    def test_windows_runner_compiles_the_bindings_and_builds_the_form(self) -> None:
        workflow = WINDOWS.read_text(encoding="utf-8")
        self.assertIn("runs-on: windows-2025", workflow)
        self.assertIn("shell: powershell", workflow)
        self.assertIn("run: scripts/test-bv-product-e2e-challenge.ps1", workflow)


if __name__ == "__main__":
    result = unittest.main(argv=[sys.argv[0]], exit=False).result
    raise SystemExit(0 if result.wasSuccessful() else 1)
