#!/usr/bin/env python3
"""The app pacing reader accepts the app's two lines or lane records, reconciles their counters and gates nothing.
The Swift writer, T17 reader and Python grammar share the fields and sample records pinned here."""

from __future__ import annotations

import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
LIVE_GATES = ROOT / "scripts/live-gates"
sys.path.insert(0, str(LIVE_GATES))
import app_pacing_diagnostics as READER  # noqa: E402
import hvf_app_pacing as APP  # noqa: E402
import hvf_host_display as EXPORTER  # noqa: E402

HELPER = str(LIVE_GATES / "app_pacing_diagnostics.py")
ENGINE = ROOT / "apps/macos/Sources/BridgeVMControl/HvfEngine"
# The lines the Swift suites print for their scenarios.
DISPLAY = ("app display pacing: ticks=12 presented=6 repeated=6 busy=0 unmapped=0 iosurface=0 first_frames=1 skipped=0 "
           "period_min_us=15625 period_max_us=15625 missed_vsyncs=0 max_tick_gap_ms=15 hold_1=0 hold_2=5 hold_3=0 "
           "hold_4=0 hold_5=0 hold_6=0 hold_gt_6=0 age_le_8ms=0 age_le_17ms=0 age_le_33ms=5 age_le_50ms=0 "
           "age_le_100ms=0 age_gt_100ms=0 age_unknown=0 max_age_us=23437 max_work_us=0 stalls_gt_50ms=0 max_stall_ms=0")
INPUT = ("app ordered input: queued=3 coalesced=1 legacy=0 refused=0 sent=2 inserted=2 discarded=0 depth_1=1 depth_2=2 "
         "depth_le_4=0 depth_le_16=0 depth_gt_16=0 max_depth=2 rtt_le_10ms=0 rtt_le_20ms=1 rtt_le_50ms=1 "
         "rtt_le_100ms=0 rtt_le_250ms=0 rtt_gt_250ms=0 max_rtt_ms=40")
FILE = DISPLAY + "\n" + INPUT + "\n"
# A busier session: uneven holds, unknown and old ages, stalls, coalescing and a slow round trip.
BUSY = ("app display pacing: ticks=1000 presented=400 repeated=560 busy=3 unmapped=30 iosurface=7 first_frames=2 "
        "skipped=5 period_min_us=8333 period_max_us=16666 missed_vsyncs=9 max_tick_gap_ms=120 hold_1=40 hold_2=300 "
        "hold_3=50 hold_4=5 hold_5=1 hold_6=0 hold_gt_6=2 age_le_8ms=100 age_le_17ms=200 age_le_33ms=80 "
        "age_le_50ms=10 age_le_100ms=4 age_gt_100ms=1 age_unknown=3 max_age_us=180000 max_work_us=900 "
        "stalls_gt_50ms=2 max_stall_ms=120")
BUSY_INPUT = ("app ordered input: queued=50 coalesced=20 legacy=1 refused=2 sent=30 inserted=29 discarded=1 depth_1=10 "
              "depth_2=39 depth_le_4=1 depth_le_16=0 depth_gt_16=0 max_depth=3 rtt_le_10ms=0 rtt_le_20ms=9 "
              "rtt_le_50ms=15 rtt_le_100ms=4 rtt_le_250ms=0 rtt_gt_250ms=1 max_rtt_ms=400")
UNRECONCILED = (
    (DISPLAY, "ticks=12", "ticks=13", "tick outcomes do not sum to ticks"),
    (DISPLAY, "first_frames=1", "first_frames=7", "more first frames than new frames"),
    (DISPLAY, "hold_2=5", "hold_2=4", "holds do not count every new frame after a first frame"),
    (DISPLAY, "age_le_33ms=5", "age_le_33ms=6", "ages do not count every new frame after a first frame"),
    (DISPLAY, "max_age_us=23437", "max_age_us=17000", "the oldest frame is outside its age bucket"),
    (DISPLAY, "max_age_us=23437", "max_age_us=33001", "the oldest frame is outside its age bucket"),
    (DISPLAY, "max_age_us=23437", "max_age_us=0", "the oldest frame is outside its age bucket"),
    (DISPLAY, "period_min_us=15625", "period_min_us=16000", "the frame period range is not ordered"),
    (DISPLAY, "period_min_us=15625", "period_min_us=0", "the frame period range is not ordered"),
    (DISPLAY, "missed_vsyncs=0 max_tick_gap_ms=15", "missed_vsyncs=1 max_tick_gap_ms=0", "vsyncs were missed without a tick gap"),
    (DISPLAY, "stalls_gt_50ms=0", "stalls_gt_50ms=1", "stalls do not fit the longest main-thread stretch"),
    (DISPLAY, "max_stall_ms=0", "max_stall_ms=51", "stalls do not fit the longest main-thread stretch"),
    (INPUT, "depth_2=2", "depth_2=3", "depths do not count every queued event"),
    (INPUT, "coalesced=1", "coalesced=4", "more coalesced than queued events"),
    (INPUT, "max_depth=2", "max_depth=5", "the deepest queue is outside its depth bucket"),
    (INPUT, "max_depth=2", "max_depth=0", "the deepest queue is outside its depth bucket"),
    (INPUT, "sent=2", "sent=1", "more inserted than sent events"),
    (INPUT, "rtt_le_50ms=1", "rtt_le_50ms=0", "round trips do not count every inserted event"),
    (INPUT, "max_rtt_ms=40", "max_rtt_ms=20", "the longest round trip is outside its bucket"),
    (INPUT, "max_rtt_ms=40", "max_rtt_ms=51", "the longest round trip is outside its bucket"),
)
MALFORMED = {
    "a leading zero": DISPLAY.replace("ticks=12", "ticks=012"),
    "a missing field": DISPLAY.replace(" busy=0", ""),
    "an extra field": DISPLAY + " extra=1",
    "two fields swapped": DISPLAY.replace("repeated=6 busy=0", "busy=0 repeated=6"),
    "a value beyond u64": INPUT.replace("discarded=0", "discarded=18446744073709551616"),
    "a negative value": INPUT.replace("discarded=0", "discarded=-1"),
    "a tab": INPUT.replace(" sent=2", "\tsent=2"),
}


def run(*args: str) -> subprocess.CompletedProcess:
    return subprocess.run([sys.executable, HELPER, *args], cwd=ROOT, capture_output=True, text=True, timeout=60)


def swift_fields(path: Path) -> tuple[str, list[str]]:
    text = path.read_text(encoding="utf-8")
    prefix = re.search(r'static let prefix = "([^"]+)"', text).group(1)
    body = re.search(r"static let fields = \[(.*?)\]", text, re.S).group(1)
    return prefix, re.findall(r'"([a-z0-9_]+)"', body)


class AppPacingReader(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)

    def tearDown(self):
        self.temporary.cleanup()

    def write(self, name: str, text: str) -> str:
        path = self.root / name
        path.write_text(text, encoding="utf-8")
        return str(path)

    def test_the_swift_writer_and_the_grammar_name_the_same_fields(self):
        self.assertEqual(swift_fields(ENGINE / "HvfDisplayPacingRecord.swift"), (APP.DISPLAY_PREFIX, list(APP.DISPLAY_FIELDS)))
        self.assertEqual(swift_fields(ENGINE / "HvfOrderedInputRecord.swift"), (APP.INPUT_PREFIX, list(APP.INPUT_FIELDS)))
        reader = (ROOT / "apps/macos/Sources/BridgeVMProductE2E/T17FirstRunAppPacing.swift").read_text(encoding="utf-8")
        block = re.search(r"static let records[^=]*= \[(.*?)\n    \]", reader, re.S).group(1)
        listed = {prefix: re.findall(r'"([a-z0-9_]+)"', body)
                  for prefix, body in re.findall(r'\("([^"]+)", \[(.*?)\]\)', block, re.S)}
        self.assertEqual(listed, {prefix: list(fields) for prefix, fields in APP.RECORDS.items()})

    def test_every_sample_reconciles_and_prints_back(self):
        for line in (DISPLAY, INPUT, BUSY, BUSY_INPUT):
            prefix = next(name for name in APP.RECORDS if line.startswith(name))
            with self.subTest(prefix=prefix, line=line[:40]):
                self.assertEqual(APP.record(prefix, APP.parse(prefix, line)), line)
        self.assertEqual(APP.read_file(FILE.encode()), {APP.DISPLAY_PREFIX: APP.parse(APP.DISPLAY_PREFIX, DISPLAY),
                                                        APP.INPUT_PREFIX: APP.parse(APP.INPUT_PREFIX, INPUT)})
        zero = {prefix: dict.fromkeys(fields, 0) for prefix, fields in APP.RECORDS.items()}
        for prefix, values in zero.items():
            self.assertEqual(APP.reconcile(prefix, values), values, "an empty record reconciles")

    def test_records_that_do_not_reconcile_or_parse_are_refused(self):
        for line, old, new, reason in UNRECONCILED:
            prefix = next(name for name in APP.RECORDS if line.startswith(name))
            with self.subTest(new=new), self.assertRaisesRegex(APP.PacingError, re.escape(reason)):
                self.assertIn(old, line)
                APP.parse(prefix, line.replace(old, new, 1))
        for name, line in MALFORMED.items():
            prefix = next(prefix for prefix in APP.RECORDS if line.startswith(prefix))
            with self.subTest(name), self.assertRaisesRegex(APP.PacingError, "record is malformed"):
                APP.parse(prefix, line)
        for name, text in {"one line": DISPLAY + "\n", "reordered": INPUT + "\n" + DISPLAY + "\n", "no newline": FILE[:-1],
                           "a third line": FILE + "x\n", "not ascii": FILE.replace("ticks", "tícks"), "empty": ""}.items():
            with self.subTest(name), self.assertRaisesRegex(APP.PacingError, "not the app's pacing records"):
                APP.read_file(text.encode())

    def test_the_cli_prints_records_and_a_summary(self):
        reported = run(self.write("app-pacing.log", BUSY + "\n" + BUSY_INPUT + "\n"))
        self.assertEqual((reported.returncode, reported.stderr), (0, ""))
        lines = reported.stdout.splitlines()
        self.assertEqual(lines[:2], [BUSY, BUSY_INPUT])
        self.assertIn("display: 1000 ticks, frame period 8.333-16.666 ms (60.0-120.0 Hz); 9 vsyncs missed, longest tick "
                      "gap 120 ms", lines)
        self.assertIn("display holds in vsyncs: 1:10.1% 2:75.4% 3:12.6% 4:1.3% 5:0.3% 6:0.0% >6:0.5% of 398", lines)
        self.assertIn("display content age at its target vsync: <=8ms:25.3% <=17ms:50.6% <=33ms:20.3% <=50ms:2.5% <=100ms:1.0% "
                      ">100ms:0.3% of 395, unknown 3, oldest 180.000 ms", lines)
        self.assertIn("main thread: 2 stalls over 50 ms, longest stretch 120 ms; longest display tick 0.900 ms", lines)
        self.assertIn("ordered input round trip: <=10ms:0.0% <=20ms:31.0% <=50ms:51.7% <=100ms:13.8% <=250ms:0.0% "
                      ">250ms:3.4% of 29, longest 400 ms", lines)
        fields = json.loads(run("--json", self.write("json.log", FILE)).stdout)
        self.assertEqual({prefix: list(values) for prefix, values in fields.items()},
                         {prefix: list(names) for prefix, names in APP.RECORDS.items()})

    def test_a_lane_result_reads_beside_the_exporter_record(self):
        records = {APP.lane_name(prefix): APP.parse(prefix, line) for prefix, line in zip(APP.RECORDS, (DISPLAY, INPUT))}
        timing = dict.fromkeys(EXPORTER.DISPLAY_FIELDS, 0) | {"ticks": 200, "late_le_1ms": 197, "published": 100,
                                                               "interval_le_40ms": 99}
        lane = {"schema_version": "fixture", "first_run_host_records": {**records, "hda_poll": {}, "ramfb_display_timing": timing}}
        reported = run(self.write("lane.json", json.dumps(lane)))
        self.assertEqual((reported.returncode, reported.stderr), (0, ""))
        self.assertEqual(reported.stdout.splitlines()[:2], [DISPLAY, INPUT])
        self.assertIn("exporter (whole boot): 200 ticks, 98.5% woke within 1 ms; 100 published, intervals <=20ms:0.0% "
                      "<=40ms:100.0% <=70ms:0.0% >70ms:0.0% of 99", reported.stdout.splitlines())
        alone = run("--json", self.write("alone.json", json.dumps({"first_run_host_records": records})))
        self.assertEqual(list(json.loads(alone.stdout)), list(APP.RECORDS))

    def test_nothing_else_reads_as_records(self):
        good = self.write("good.log", FILE)
        link = self.root / "link.log"
        link.symlink_to(good)
        fifo = self.root / "fifo"
        os.mkfifo(fifo)
        lanes = {"no records": {"first_run_host_records": {"hda_poll": {}}}, "no key": {"lane": 1},
                 "a malformed record": {"first_run_host_records": {APP.lane_name(APP.DISPLAY_PREFIX): {"ticks": 1},
                                                                    APP.lane_name(APP.INPUT_PREFIX): {}}},
                 "a malformed exporter record": {"first_run_host_records": {
                     **{APP.lane_name(p): dict.fromkeys(f, 0) for p, f in APP.RECORDS.items()},
                     "ramfb_display_timing": {"ticks": "1"}}}}
        lanes.update({f"{prefix} invalid shape {index}": {"first_run_host_records": {
            APP.lane_name(p): value if p == prefix else dict.fromkeys(f, 0) for p, f in APP.RECORDS.items()}}
            for prefix in APP.RECORDS for index, value in enumerate((None, 7, True, [], list(APP.RECORDS[prefix])))})
        cases = {"missing": str(self.root / "missing.log"), "a link": str(link), "a fifo": str(fifo),
                 "an old run": self.write("old.log", "boot\n"), "not json": self.write("bad.json", "{nope"),
                 "unreconciled": self.write("bad.log", FILE.replace("sent=2", "sent=1")),
                 **{name: self.write(name.replace(" ", "-") + ".json", json.dumps(lane)) for name, lane in lanes.items()}}
        for name, path in cases.items():
            with self.subTest(name):
                refused = run(path)
                self.assertEqual((refused.stdout, refused.returncode), ("", 1))
                self.assertTrue(refused.stderr.startswith("app pacing diagnostics: "), refused.stderr)
        for usage in ((), ("--json",), ("--csv", good), (good, good)):
            self.assertEqual(run(*usage).returncode, 2, usage)


if __name__ == "__main__":
    unittest.main(verbosity=2)
