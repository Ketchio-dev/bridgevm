#!/usr/bin/env python3
"""A T17 journey failure after first READY freezes the guest before the app stops, so its screen survives."""
from __future__ import annotations

import sys
import unittest
from pathlib import Path

E2E = Path(__file__).resolve().parents[2] / "apps/macos/Sources/BridgeVMProductE2E"


class PostReadyFreeze(unittest.TestCase):
    def test_only_a_failure_after_first_ready_freezes_the_guest_before_the_app_stops(self) -> None:
        run = (E2E / "T17ProductRunner.swift").read_text(encoding="utf-8").split("func run() -> T17RunOutcome {", 1)[1]
        run = run.split("\n    }\n", 1)[0]
        self.assertEqual(run.count("afterReady = true"), 1)
        self.assertLess(run.index("try evidence.prove(.firstReady); afterReady = true"), run.index("try T17GuestJourney("))
        freeze = 'detail = blocker.detail + (afterReady ? "; " + freezeAfterFirstReady() : "")'
        self.assertEqual(run.count(freeze), 1)
        self.assertLess(run.index(freeze), run.index("let clean = stopOwnedApplication()"))

    def test_the_freeze_is_the_host_diagnostic_stop_on_the_owned_run_log(self) -> None:
        source = (E2E / "T17ProductRunnerFirstReady.swift").read_text(encoding="utf-8")
        freeze = source.split("func freezeAfterFirstReady() -> String {", 1)[1].split("\n    }\n", 1)[0]
        self.assertIn('T17FirstReadyStopCapture.capture(log: bundle.appendingPathComponent("logs/hvf/run.log")', freeze)
        self.assertIn("laneRoot: URL(fileURLWithPath: request.laneRoot)", freeze)


if __name__ == "__main__":
    result = unittest.main(argv=[sys.argv[0]], exit=False).result
    raise SystemExit(0 if result.wasSuccessful() else 1)
