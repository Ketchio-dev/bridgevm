#!/usr/bin/env python3
"""The shared T17/T19 guest journey reads the guest TPM once at each of its three
agent-ready boots, never through try; both product runners stage the TPM assets best
effort and pass their evidence log; the helper adds that log beside each lane result."""
from __future__ import annotations

from pathlib import Path
import re
import sys
import unittest

ROOT = Path(__file__).resolve().parents[2]
E2E = ROOT / "apps/macos/Sources/BridgeVMProductE2E"


def source(name: str) -> str:
    return (E2E / name).read_text(encoding="utf-8")


class TpmJourneyContract(unittest.TestCase):
    def test_journey_reads_each_agent_ready_boot_once_and_never_through_try(self) -> None:
        journey = source("T17GuestJourney.swift")
        run = journey[journey.index("    func run("):journey.index("    private func keyboardAndPointer")]
        boots = run.split("try bootReady()")
        self.assertEqual([re.findall(r"readTPM\(\.(\w+)\)", boot) for boot in boots],
                         [["first"], ["mutation"], ["final"]])
        self.assertLess(boots[0].index("try keyboardAndPointer()"), boots[0].index("readTPM(.first)"))
        self.assertNotRegex(run, r"try\s+readTPM")

    def test_both_runners_stage_the_assets_and_pass_their_log(self) -> None:
        for name in ("T17ProductRunner.swift", "A9ImportProductRunner.swift"):
            with self.subTest(runner=name):
                runner = source(name)
                self.assertEqual(runner.count("let tpm = T17TPMEvidenceLog()"), 1)
                self.assertEqual(runner.count("T17GuestJourney(request: request, ui: control, fileManager: fileManager, tpm: tpm)"), 1)
                stage = "; T17TPMGuestAssets.stage(appBundlePath: request.appBundlePath, sharePath: request.sharePath, fileManager: fileManager)\n"
                self.assertEqual(runner.count("try fileManager.createDirectory(atPath: request.sharePath, withIntermediateDirectories: false)" + stage), 1)
        self.assertIn("    static func stage(appBundlePath: String, sharePath: String, fileManager: FileManager = .default) {\n",
                      source("T17TPMGuestAssets.swift"))

    def test_helper_writes_the_evidence_beside_both_lane_results(self) -> None:
        main = source("BridgeVMProductE2EMain.swift")
        self.assertEqual(main.count("try T17ResultWriter.write(ProductE2EPrivateLaneResult(lane: T17FirstRunHostRecords.attach(outcome.evidence.result("), 2)
        self.assertEqual(main.count("tpm: runner.tpm.evidence), to: cli.result)"), 2)
        self.assertNotIn("T17ResultWriter.write(outcome", main)


if __name__ == "__main__":
    result = unittest.main(argv=[sys.argv[0]], exit=False).result
    raise SystemExit(0 if result.wasSuccessful() else 1)
