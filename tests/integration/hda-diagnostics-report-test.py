#!/usr/bin/env python3
"""hda_diagnostics.py reports the host's HDA diagnostic records and gates nothing.

It reads them only from the bound final report's host tail, accepts every record
the Rust tests pin, refuses one whose counters do not reconcile, and reads a log
from before the records as having none. The continuity helper reads logs with
and without them identically; hda-diagnostics-host-tail-contract.py checks
every other reader.
"""

from __future__ import annotations

import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
LIVE_GATES = ROOT / "scripts/live-gates"
sys.path.insert(0, str(LIVE_GATES))
import hda_continuity  # noqa: E402
import hda_diagnostics  # noqa: E402

SPEC = importlib.util.spec_from_file_location("hda_diagnostics_contract", ROOT / "tests/integration/hda-diagnostics-host-tail-contract.py")
CONTRACT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CONTRACT)
READERS, HELPER = CONTRACT.READERS, str(LIVE_GATES / "hda_diagnostics.py")
POLL, STREAM, REPEAT, TIMING, DIAG, TEARDOWN = (CONTRACT.POLL, CONTRACT.STREAM, CONTRACT.REPEAT, CONTRACT.TIMING,
                                                 CONTRACT.DIAG, CONTRACT.TEARDOWN)
# Records the Rust tests pin as printed (hda/tests/diagnostics.rs, repeat_detector.rs, hda_coreaudio_timing_tests.rs).
PRINTED = (
    "hda poll: polls=3 gap_le_10ms=2 gap_le_20ms=0 gap_le_50ms=0 gap_le_100ms=0 gap_gt_100ms=1 max_gap_ms=250 "
    "capped_polls=1 capped_lost_ms=150 ioc_completions=11 max_iocs_per_poll=10 msis_sent=0",
    "hda poll: polls=0 gap_le_10ms=0 gap_le_20ms=0 gap_le_50ms=0 gap_le_100ms=0 gap_gt_100ms=0 max_gap_ms=0 "
    "capped_polls=0 capped_lost_ms=0 ioc_completions=0 max_iocs_per_poll=0 msis_sent=0",
    "hda stream: runs=1 rate_hz=48000 channels=2 bits=16 cbl_bytes=5800 cbl_ms=30.208 entries=3 entry_bytes_min=1000 "
    "entry_bytes_max=2880 entry_ms_min=5.208 entry_ms_max=15.000 ioc_entries=1 position_buffer=1 lpib_reads=3 running_ms=4",
    "hda stream: runs=2 rate_hz=48000 channels=2 bits=16 cbl_bytes=3840 cbl_ms=20.000 entries=1 entry_bytes_min=3840 "
    "entry_bytes_max=3840 entry_ms_min=20.000 entry_ms_max=20.000 ioc_entries=1 position_buffer=0 lpib_reads=0 running_ms=15",
    "hda stream: runs=0 rate_hz=0 channels=0 bits=0 cbl_bytes=0 cbl_ms=0.000 entries=0 entry_bytes_min=0 "
    "entry_bytes_max=0 entry_ms_min=0.000 entry_ms_max=0.000 ioc_entries=0 position_buffer=0 lpib_reads=0 running_ms=0",
    *(f"hda repeat: passes={p} silent_passes={s} compared_passes={c} repeat_passes={r} max_repeat_run={m}"
      for p, s, c, r, m in ((0, 0, 0, 0, 0), (4, 0, 2, 2, 2), (4, 0, 2, 0, 0), (4, 0, 2, 1, 1), (6, 6, 0, 0, 0), (9, 0, 7, 3, 1))),
    "hda CoreAudio timing: callbacks=10 interval_le_10ms=1 interval_le_15ms=2 interval_le_20ms=1 interval_le_30ms=2 "
    "interval_le_50ms=2 interval_gt_50ms=1 max_interval_ms=51 late_callbacks=3 trylock_misses=0 ring_samples=0 "
    "ring_min_ms=0 ring_max_ms=0 ring_final_ms=0",
    "hda CoreAudio timing: callbacks=0 interval_le_10ms=0 interval_le_15ms=0 interval_le_20ms=0 interval_le_30ms=0 "
    "interval_le_50ms=0 interval_gt_50ms=0 max_interval_ms=0 late_callbacks=0 trylock_misses=0 ring_samples=0 "
    "ring_min_ms=0 ring_max_ms=0 ring_final_ms=0",
)
UNRECONCILED = (
    (POLL, "polls=3", "polls=4", "gap buckets do not sum to polls"),
    (POLL, "capped_polls=1", "capped_polls=0", "capped polls are not the gaps over 100 ms"),
    (POLL.replace("gap_le_10ms=2", "gap_le_10ms=3").replace("gap_gt_100ms=1", "gap_gt_100ms=0").replace(
        "max_gap_ms=250", "max_gap_ms=9"), "capped_polls=1", "capped_polls=0", "time was lost without a capped poll"),
    (POLL, "max_gap_ms=250", "max_gap_ms=99", "the longest gap is outside its bucket"),
    (POLL.replace("gap_le_10ms=2 gap_le_20ms=0", "gap_le_10ms=1 gap_le_20ms=1").replace("gap_gt_100ms=1", "gap_gt_100ms=0")
     .replace("capped_polls=1 capped_lost_ms=150", "capped_polls=0 capped_lost_ms=0").replace("polls=3", "polls=2"),
     "max_gap_ms=250", "max_gap_ms=21", "the longest gap is outside its bucket"),
    (POLL, "max_iocs_per_poll=10", "max_iocs_per_poll=12", "IOCs per poll do not fit the IOC count"),
    (POLL, "ioc_completions=11", "ioc_completions=0", "IOCs per poll do not fit the IOC count"),
    (STREAM, "runs=1", "runs=0", "a stream shape without a run"),
    (STREAM, "ioc_entries=2", "ioc_entries=3", "more IOC entries than entries"),
    (STREAM, "entry_bytes_min=1920", "entry_bytes_min=1921", "entry bytes min exceeds max"),
    (STREAM, "entry_ms_min=10.000", "entry_ms_min=10.001", "entry ms min exceeds max"),
    (REPEAT, "repeat_passes=3", "repeat_passes=9", "more repeats than compared passes"),
    (REPEAT, "compared_passes=8", "compared_passes=11", "compared and silent passes exceed passes"),
    (REPEAT, "max_repeat_run=2", "max_repeat_run=4", "the longest repeat run does not fit the repeats"),
    (REPEAT, "max_repeat_run=2", "max_repeat_run=0", "the longest repeat run does not fit the repeats"),
    (TIMING, "callbacks=10", "callbacks=11", "interval buckets do not count every later callback"),
    (TIMING, "late_callbacks=3", "late_callbacks=2", "late callbacks are not the intervals over 30 ms"),
    (TIMING, "max_interval_ms=51", "max_interval_ms=49", "the longest interval is outside its bucket"),
    (TIMING, "trylock_misses=0", "trylock_misses=11", "more try_lock misses than callbacks"),
    (TIMING, "ring_min_ms=30", "ring_min_ms=31", "ring levels do not order as min, final, max"),
    (TIMING, "ring_samples=4", "ring_samples=0", "ring levels do not order as min, final, max"),
)


def prefix(line: str) -> str:
    return next(name for name in hda_diagnostics.RECORDS if line.startswith(name))


def run(*args: str) -> subprocess.CompletedProcess:
    return subprocess.run([sys.executable, HELPER, *args], cwd=ROOT, capture_output=True, text=True, timeout=60)


class DiagnosticsReportTest(unittest.TestCase):
    def test_helper_reports_the_records_from_the_host_tail_only(self):
        found = hda_diagnostics.diagnostics(CONTRACT.audio_log(DIAG + TEARDOWN).encode())
        self.assertEqual(list(found), list(hda_diagnostics.RECORDS), "print order")
        self.assertEqual([hda_diagnostics.record(name, values) for name, values in found.items()], list(CONTRACT.RECORDS))
        self.assertEqual((found["hda poll:"]["capped_lost_ms"], found["hda stream:"]["cbl_ms"]), (150, "20.000"))
        alone = hda_diagnostics.diagnostics(CONTRACT.audio_log(TIMING + "\n" + TEARDOWN).encode())
        self.assertEqual(list(alone), ["hda CoreAudio timing:"])
        for raw, reason in ((READERS.honest(serial=("boot\r\n" + DIAG.replace("\n", "\r\n")).encode(),
                                            tail=TEARDOWN.encode()), "no HDA diagnostic record"),
                            (READERS.AGENT.replace("bridgevm\\user", POLL).encode()
                             + READERS.honest(tail=TEARDOWN.encode())[len(READERS.AGENT):], "no HDA diagnostic record"),
                            ((READERS.AGENT + DIAG + TEARDOWN).encode(), "no final host report"),
                            (CONTRACT.audio_log(TEARDOWN + DIAG).encode(), "no final host report"),
                            (CONTRACT.audio_log(POLL + "\n" + DIAG + TEARDOWN).encode(), "no final host report")):
            with self.subTest(reason), self.assertRaisesRegex(hda_diagnostics.DiagnosticsError, reason):
                hda_diagnostics.diagnostics(raw)

    def test_old_logs_read_exactly_as_before(self):
        with tempfile.TemporaryDirectory() as temp:
            new, old, link = Path(temp) / "run.log", Path(temp) / "old.log", Path(temp) / "link.log"
            new.write_text(CONTRACT.audio_log(DIAG + TEARDOWN), encoding="utf-8")
            old.write_text(CONTRACT.audio_log(TEARDOWN), encoding="utf-8")
            link.symlink_to(new)
            reported = run(str(new))
            self.assertEqual((reported.stdout, reported.stderr, reported.returncode), (DIAG, "", 0))
            fields = json.loads(run("--json", str(new)).stdout)
            self.assertEqual({name: list(values) for name, values in fields.items()},
                             {name: list(names) for name, names in hda_diagnostics.RECORDS.items()})
            absent = run(str(old))
            self.assertEqual((absent.stdout, absent.returncode), ("", 1))
            self.assertIn("the host tail has no HDA diagnostic record", absent.stderr)
            self.assertEqual(run(str(link)).returncode, 1, "a linked run.log reads as empty")
            for usage in ((), ("--json",), ("--csv", str(new)), (str(new), str(old))):
                self.assertEqual(run(*usage).returncode, 2, usage)
            continuity = [subprocess.run([sys.executable, str(LIVE_GATES / "hda_continuity.py"), *args, str(log)],
                                         capture_output=True, text=True, timeout=60) for log in (old, new)
                          for args in ((), ("--json",))]
            self.assertEqual([(c.stdout, c.returncode) for c in continuity[:2]],
                             [(c.stdout, c.returncode) for c in continuity[2:]], "hda_continuity reads both alike")
            self.assertEqual(continuity[0].stdout, CONTRACT.CONTINUITY)
            self.assertEqual(hda_continuity.continuity(old.read_bytes()), hda_continuity.continuity(new.read_bytes()))

    def test_every_record_the_host_prints_reconciles(self):
        for line in (*PRINTED, *CONTRACT.RECORDS):
            with self.subTest(line):
                values = hda_diagnostics.parse(prefix(line), line)
                self.assertEqual(hda_diagnostics.record(prefix(line), values), line)

    def test_records_that_do_not_reconcile_or_parse_are_refused(self):
        for line, old, new, reason in UNRECONCILED:
            with self.subTest(new=new, reason=reason), self.assertRaisesRegex(hda_diagnostics.DiagnosticsError, reason):
                self.assertIn(old, line)
                hda_diagnostics.parse(prefix(line), line.replace(old, new, 1))
        for name, line in CONTRACT.MALFORMED.items():
            with self.subTest(name), self.assertRaisesRegex(hda_diagnostics.DiagnosticsError, "record is malformed"):
                hda_diagnostics.parse(prefix(line), line)
        with tempfile.TemporaryDirectory() as temp:
            log = Path(temp) / "run.log"
            log.write_text(CONTRACT.audio_log(DIAG.replace("late_callbacks=3", "late_callbacks=2") + TEARDOWN), encoding="utf-8")
            refused = run(str(log))
            self.assertEqual((refused.stdout, refused.returncode), ("", 1))
            self.assertIn("hda CoreAudio timing: late callbacks are not the intervals over 30 ms", refused.stderr)


if __name__ == "__main__":
    unittest.main(verbosity=2)
