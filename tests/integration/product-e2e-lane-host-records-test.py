#!/usr/bin/env python3
"""T17 and T19 lane results may carry the first run's host records; both lane checks admit them only well formed.

BridgeVMProductE2E copies the HDA diagnostic records of the first run's bound
post-footer host tail into its private lane result as integer fields under
first_run_host_records (T17FirstRunHostRecords.swift), and leaves the key out
when the host printed none. Both receipt writers admit a lane without the key
exactly as before, admit the key only when every record and field is one the
host prints, and never copy it into a receipt. T17FirstRunHostRecordsTests.swift
pins the same values on the Swift side.
"""
from __future__ import annotations

import json
from pathlib import Path
import sys
import tempfile
import unittest

import product_e2e_identity_fixtures as fixture
from product_e2e_import_fixture_base import prepare

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/live-gates"))
import product_e2e_host_records as HOST_RECORDS  # noqa: E402

KEY = "first_run_host_records"
# The values T17FirstRunHostRecordsTests.swift parses from its four sample records.
RECORDS = {
    "hda_poll": {"polls": 3, "gap_le_10ms": 2, "gap_le_20ms": 0, "gap_le_50ms": 0, "gap_le_100ms": 0, "gap_gt_100ms": 1,
                 "max_gap_ms": 250, "capped_polls": 1, "capped_lost_ms": 150, "ioc_completions": 11,
                 "max_iocs_per_poll": 10, "msis_sent": 1},
    "hda_stream": {"runs": 1, "rate_hz": 48000, "channels": 2, "bits": 16, "cbl_bytes": 3840, "cbl_us": 21333,
                   "entries": 2, "entry_bytes_min": 1920, "entry_bytes_max": 1920, "entry_us_min": 10000,
                   "entry_us_max": 10666, "ioc_entries": 2, "position_buffer": 1, "lpib_reads": 7, "running_ms": 260},
    "hda_repeat": {"passes": 12, "silent_passes": 2, "compared_passes": 8, "repeat_passes": 3, "max_repeat_run": 2},
    "hda_coreaudio_timing": {"callbacks": 10, "interval_le_10ms": 1, "interval_le_15ms": 2, "interval_le_20ms": 1,
                             "interval_le_30ms": 2, "interval_le_50ms": 2, "interval_gt_50ms": 1, "max_interval_ms": 51,
                             "late_callbacks": 3, "trylock_misses": 0, "ring_samples": 4, "ring_min_ms": 30,
                             "ring_max_ms": 50, "ring_final_ms": 2 ** 64 - 1},
}


def malformed() -> dict[str, object]:
    def changed(name: str, field: str, value: object) -> dict:
        return {**RECORDS, name: {**RECORDS[name], field: value}}
    extra = {**RECORDS, "hda_poll": {**RECORDS["hda_poll"], "dropouts": 0}}
    missing = {**RECORDS, "hda_poll": {k: v for k, v in RECORDS["hda_poll"].items() if k != "msis_sent"}}
    return {
        "an empty object": {}, "a list": [RECORDS["hda_poll"]], "a string": "hda poll: polls=3",
        "an unknown record": {**RECORDS, "hda_dropouts": {"count": 1}},
        "a record that is not an object": {**RECORDS, "hda_repeat": [12, 2, 8, 3, 2]},
        "an extra field": extra, "a missing field": missing,
        "the record's millisecond spelling": {**RECORDS, "hda_stream": {
            ("cbl_ms" if k == "cbl_us" else k): v for k, v in RECORDS["hda_stream"].items()}},
        "a negative value": changed("hda_poll", "polls", -1),
        "a value beyond u64": changed("hda_poll", "polls", 2 ** 64),
        "a boolean": changed("hda_stream", "position_buffer", True),
        "a float": changed("hda_repeat", "passes", 12.0),
        "a string value": changed("hda_repeat", "passes", "12"),
        "a null value": changed("hda_repeat", "passes", None),
    }


class LaneHostRecords(unittest.TestCase):
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

    def test_both_lane_checks_admit_the_records_and_a_lane_without_them_as_before(self):
        for writer in (fixture.T17, fixture.T19):
            with self.subTest(tier=writer.__name__):
                base = fixture.lane(writer)
                self.assertEqual(self.check(writer, base), base)
                carried = {**base, KEY: RECORDS}
                self.assertEqual(self.check(writer, carried), carried)
                for name, records in RECORDS.items():
                    one = {**base, KEY: {name: records}}
                    self.assertEqual(self.check(writer, one), one, name)

    def test_both_lane_checks_refuse_malformed_records(self):
        for writer in (fixture.T17, fixture.T19):
            for name, records in malformed().items():
                with self.subTest(tier=writer.__name__, case=name), self.assertRaises(ValueError):
                    self.check(writer, {**fixture.lane(writer), KEY: records})
            with self.subTest(tier=writer.__name__, case="another unknown key"), self.assertRaises(ValueError):
                self.check(writer, {**fixture.lane(writer), "host_records": RECORDS})

    def test_the_lane_records_are_the_hda_grammar_with_milliseconds_as_microseconds(self):
        self.assertEqual({**{name: frozenset(fields) for name, fields in RECORDS.items()}, "ramfb_display_timing": frozenset(HOST_RECORDS.DISPLAY.DISPLAY_FIELDS)}, HOST_RECORDS.LANE_RECORDS)
        self.assertEqual(HOST_RECORDS.KEY, KEY)

    def test_receipt_writers_never_copy_the_records(self):
        for script in ("write-windows-product-e2e-receipt.py", "write-windows-import-product-e2e-receipt.py"):
            text = (ROOT / "scripts/live-gates" / script).read_text(encoding="utf-8")
            self.assertEqual(text.count("HOST_RECORDS."), 1, script)
            self.assertIn("HOST_RECORDS.lane_keys(value) - PRIVATE_LANE.KEYS != LANE_KEYS", text, script)
            self.assertNotIn(KEY, text, script)


class ImportLaneAuthentication(unittest.TestCase):
    """A complete T19 lane still authenticates when it carries the records."""

    def setUp(self):
        prepare(self, "bridgevm-import-host-records-")

    def tearDown(self):
        self.temporary.cleanup()

    def test_a_complete_import_lane_with_records_authenticates(self):
        result = json.loads(self.result_path.read_text())
        self.assertNotIn(KEY, result)
        self.result_path.write_text(json.dumps({**result, KEY: RECORDS}))
        fixture.T19.authenticate(self.request_path, self.result_path, self.stamp, fixture.JOB, fixture.COMMIT,
                                 "pilot", 1, fixture.T19.digest(self.request_path))
        self.assertTrue(self.stamp.is_file())


if __name__ == "__main__":
    unittest.main(verbosity=2)
