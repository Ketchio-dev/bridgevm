#!/usr/bin/env python3
"""T17 and T19 lane results may carry the app's pacing records of the first run; both lane checks admit them only well formed.

BridgeVMProductE2E copies the app's `app display pacing:` and `app ordered input:` records of the journey's first run
(hvf_app_pacing.py) into its private lane result as first_run_host_records.app_display_pacing and app_ordered_input,
beside the host's HDA and display timing records, and leaves them out when the app wrote none. Both receipt writers
admit them only with every field as an unsigned 64-bit integer, judge only their shape, so counters that disagree
never fail a lane, and no receipt copies them. T17FirstRunAppPacingTests.swift pins the same values on the Swift side.
"""
from __future__ import annotations

import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest

import product_e2e_identity_fixtures as fixture

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/live-gates"))
import hvf_app_pacing as APP  # noqa: E402
import product_e2e_host_records as HOST_RECORDS  # noqa: E402

SPEC = importlib.util.spec_from_file_location("lane_display_records", Path(__file__).with_name("product-e2e-lane-display-records-test.py"))
assert SPEC and SPEC.loader
DISPLAY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(DISPLAY)
READER_SPEC = importlib.util.spec_from_file_location("app_pacing_reader_test", Path(__file__).with_name("app-pacing-diagnostics-test.py"))
assert READER_SPEC and READER_SPEC.loader
READER = importlib.util.module_from_spec(READER_SPEC)
READER_SPEC.loader.exec_module(READER)

KEY = "first_run_host_records"
# The values T17FirstRunAppPacingTests.swift parses from the samples the app's Swift suites print.
PACING = {APP.lane_name(prefix): APP.parse(prefix, line) for prefix, line in zip(APP.RECORDS, (READER.DISPLAY, READER.INPUT))}
NAME = APP.lane_name(APP.DISPLAY_PREFIX)
MALFORMED = {
    "a missing field": {k: v for k, v in PACING[NAME].items() if k != "busy"},
    "an extra field": {**PACING[NAME], "dropped": 0},
    "a renamed field": {("max_age_ms" if k == "max_age_us" else k): v for k, v in PACING[NAME].items()},
    "a negative value": {**PACING[NAME], "ticks": -1},
    "a value beyond u64": {**PACING[NAME], "ticks": 2 ** 64},
    "a boolean": {**PACING[NAME], "presented": True},
    "a float": {**PACING[NAME], "max_age_us": 23437.0},
    "a string value": {**PACING[NAME], "ticks": "12"},
    "a list": list(PACING[NAME].values()),
}


class LaneAppPacing(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)

    def tearDown(self):
        self.temporary.cleanup()

    def check(self, writer, value: object) -> object:
        path = self.root / f"{writer.__name__}.json"
        path.write_text(json.dumps(value))
        if writer is fixture.T17:
            return writer.lane(path, job_id=fixture.JOB, commit=fixture.COMMIT, mode="pilot", ordinal=1)
        return writer.lane(path, fixture.JOB, fixture.COMMIT, "pilot", 1)

    def test_both_lane_checks_admit_the_records_alone_and_beside_the_host_records(self):
        host = {**DISPLAY.HDA.RECORDS, DISPLAY.NAME: DISPLAY.TIMING}
        disagreeing = {**PACING, NAME: {**PACING[NAME], "ticks": 999}}
        for writer in (fixture.T17, fixture.T19):
            for records in (PACING, {**host, **PACING}, {NAME: PACING[NAME]}, disagreeing,
                            {name: dict.fromkeys(fields, 2 ** 64 - 1) for name, fields in PACING.items()}):
                with self.subTest(tier=writer.__name__, records=sorted(records)):
                    lane = {**fixture.lane(writer), KEY: records}
                    self.assertEqual(self.check(writer, lane), lane)

    def test_both_lane_checks_refuse_a_malformed_record(self):
        for writer in (fixture.T17, fixture.T19):
            for case, values in MALFORMED.items():
                with self.subTest(tier=writer.__name__, case=case), self.assertRaises(ValueError):
                    self.check(writer, {**fixture.lane(writer), KEY: {**PACING, NAME: values}})

    def test_the_lane_records_are_the_app_grammar(self):
        for prefix, fields in APP.RECORDS.items():
            self.assertEqual(HOST_RECORDS.LANE_RECORDS[APP.lane_name(prefix)], frozenset(fields))
        self.assertEqual(sorted(PACING), ["app_display_pacing", "app_ordered_input"])

    def test_the_journey_keeps_only_its_first_sessions_file_and_never_fails_on_it(self):
        e2e = ROOT / "apps/macos/Sources/BridgeVMProductE2E"
        journey = (e2e / "T17GuestJourney.swift").read_text(encoding="utf-8")
        run = journey[journey.index("    func run("):journey.index("    private func keyboardAndPointer")]
        steps = ("let pacing = T17FirstRunAppPacing.mark(bundle: bundle)", "try keyboardAndPointer();",
                 "let first = try archiveLog(named: T17FirstRunHostRecords.archive",
                 "T17FirstRunAppPacing.archive(since: pacing, bundle: bundle)\n        try createSnapshot()")
        self.assertEqual([run.count(step) for step in steps], [1] * len(steps))
        self.assertEqual([run.index(step) for step in steps], sorted(run.index(step) for step in steps))
        self.assertEqual(journey.count("T17FirstRunAppPacing."), 2, "marked once, archived once, never with try")
        attach = (e2e / "T17FirstRunHostRecords.swift").read_text(encoding="utf-8")
        self.assertIn(".merging(T17FirstRunAppPacing.read(bundlePath: bundlePath)) { host, _ in host }", attach)


if __name__ == "__main__":
    unittest.main(verbosity=2)
