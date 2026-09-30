#!/usr/bin/env python3
"""T17 guest workloads create their data directory, name a failed action, and the host reports only a strict line."""
from __future__ import annotations

import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
GUEST = ROOT / "scripts/win-assets/bv-product-e2e.ps1"
E2E = ROOT / "apps/macos/Sources/BridgeVMProductE2E"


class GuestWorkloadErrors(unittest.TestCase):
    def test_guest_creates_its_data_directory_and_names_a_failed_action(self) -> None:
        source = GUEST.read_bytes().decode("utf-8").replace("\r\n", "\n")
        create = "New-Item -ItemType Directory -Force -Path 'C:\\ProgramData\\BridgeVM' | Out-Null\n"
        self.assertLess(source.index(create), source.index("try { switch ($Action) {"))
        self.assertLess(source.index("try { switch ($Action) {"), source.index("C:\\ProgramData\\BridgeVM\\t17-tone.wav"))
        catch = ('} } catch { Write-Exact "t17-error-$Prefix.txt" ("action=$Action error=" + $_.Exception.GetType().FullName'
                 ' + "`n"); throw }')
        self.assertEqual(source.count(catch), 1)
        self.assertLess(source.index(catch), source.index('Write-Output ("T17 action=$Action nonce=$Nonce status=PASS")'))

    def test_every_workload_wait_reports_the_strict_failure_line(self) -> None:
        output = (E2E / "T17GuestWorkloadOutput.swift").read_text(encoding="utf-8")
        self.assertIn('#"^action=[A-Za-z]{1,32} error=[A-Za-z_][A-Za-z0-9_.`+]{0,119}$"#', output)
        self.assertIn('share.appendingPathComponent("t17-error-\\(prefix).txt")', output)
        journey = (E2E / "T17GuestJourney.swift").read_text(encoding="utf-8")
        self.assertIn("try T17GuestWorkloadOutput.await(share: URL(fileURLWithPath: request.sharePath), name: name, prefix: prefix, timeout: timeout)", journey)
        self.assertIn("try T17GuestWorkloadOutput.await(", (E2E / "T17InputChallengeShare.swift").read_text(encoding="utf-8"))


if __name__ == "__main__":
    result = unittest.main(argv=[sys.argv[0]], exit=False).result
    raise SystemExit(0 if result.wasSuccessful() else 1)
