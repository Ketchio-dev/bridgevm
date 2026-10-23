#!/usr/bin/env python3
"""T17 and T19 lane results may carry the first run's display timing record; both lane checks admit it only well formed.

BridgeVMProductE2E copies the first run's `ramfb display timing:` record (hvf_host_display.py) into its private
lane result as first_run_host_records.ramfb_display_timing, beside the HDA records
(product-e2e-lane-host-records-test.py), and leaves it out when the platform printed none. Both receipt writers
admit it only with every field the probe prints as an unsigned 64-bit integer, and no receipt copies it.
T17FirstRunDisplayRecordsTests.swift pins the same values on the Swift side.
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
import hvf_host_display as DISPLAY  # noqa: E402
import product_e2e_host_records as HOST_RECORDS  # noqa: E402

SPEC = importlib.util.spec_from_file_location("lane_host_records", Path(__file__).with_name("product-e2e-lane-host-records-test.py"))
assert SPEC and SPEC.loader
HDA = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(HDA)

KEY, NAME = "first_run_host_records", "ramfb_display_timing"
# The values T17FirstRunDisplayRecordsTests.swift parses from its sample record.
TIMING = {"ticks": 4960, "late_le_1ms": 4100, "late_le_4ms": 700, "late_le_8ms": 120, "late_le_16ms": 30,
          "late_gt_16ms": 10, "max_late_us": 41250, "published": 3202, "interval_le_20ms": 0, "interval_le_40ms": 2900,
          "interval_le_70ms": 250, "interval_gt_70ms": 51, "max_interval_ms": 9050}
MALFORMED = {
    "a missing field": {k: v for k, v in TIMING.items() if k != "published"},
    "an extra field": {**TIMING, "dropped": 0},
    "a renamed field": {("max_late_ms" if k == "max_late_us" else k): v for k, v in TIMING.items()},
    "a negative value": {**TIMING, "ticks": -1},
    "a value beyond u64": {**TIMING, "ticks": 2 ** 64},
    "a boolean": {**TIMING, "published": True},
    "a float": {**TIMING, "max_interval_ms": 9050.0},
    "a string value": {**TIMING, "ticks": "4960"},
    "a list": list(TIMING.values()),
}


class LaneDisplayRecords(unittest.TestCase):
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

    def test_both_lane_checks_admit_the_record_alone_and_beside_the_hda_records(self):
        for writer in (fixture.T17, fixture.T19):
            for records in ({NAME: TIMING}, {**HDA.RECORDS, NAME: TIMING}, {NAME: dict.fromkeys(TIMING, 2 ** 64 - 1)}):
                with self.subTest(tier=writer.__name__, records=sorted(records)):
                    lane = {**fixture.lane(writer), KEY: records}
                    self.assertEqual(self.check(writer, lane), lane)

    def test_both_lane_checks_refuse_a_malformed_record(self):
        for writer in (fixture.T17, fixture.T19):
            for name, timing in MALFORMED.items():
                with self.subTest(tier=writer.__name__, case=name), self.assertRaises(ValueError):
                    self.check(writer, {**fixture.lane(writer), KEY: {**HDA.RECORDS, NAME: timing}})

    def test_the_lane_record_is_the_display_grammar_beside_the_hda_records(self):
        self.assertEqual(HOST_RECORDS.LANE_RECORDS[NAME], frozenset(DISPLAY.DISPLAY_FIELDS))
        self.assertEqual(frozenset(TIMING), frozenset(DISPLAY.DISPLAY_FIELDS))
        self.assertEqual(sorted(HOST_RECORDS.LANE_RECORDS), sorted([*HDA.RECORDS, NAME]))


if __name__ == "__main__":
    unittest.main(verbosity=2)
