#!/usr/bin/env python3
"""The ramfb display export's stop record precedes the final report, so a shutdown stays readable.

The export thread prints `ramfb display export: stopped ...` when it is joined.
Held until the probe returned, it printed after the report's footer, where every
reader admits only host audio records. T17 r52 therefore read a guest SYSTEM_OFF
as no terminal stop at all. The probe now drops the thread on the terminal path
before persist_and_report_stop!, and the readers' grammar is unchanged.
"""
from __future__ import annotations

import importlib.util
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
PROBE = ROOT / "crates/bridgevm-hvf/examples/hvf_gic_boot_probe"
sys.path.insert(0, str(ROOT / "scripts/live-gates"))
from hvf_stop_line import SYSTEM_OFF  # noqa: E402
from hvf_terminal_report import system_off_offset  # noqa: E402

SPEC = importlib.util.spec_from_file_location(
    "hvf_stop_readers_order", ROOT / "tests/integration/hvf-stop-readers-contract.py")
assert SPEC and SPEC.loader
READERS = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(READERS)
EXPORT_STOP = ("ramfb display export: stopped polls=4960 published=3202 seconds=163.7 "
               "poll_hz=30.3 publish_hz=19.6 slowest_tick_us=937\n")


class Contract(unittest.TestCase):
    def test_the_probe_joins_the_export_before_the_final_report(self):
        source = (PROBE / "probe_runtime.rs").read_text()
        terminal = source[source.rindex("continue 'reboot;"):]
        self.assertEqual(source.count("drop(ramfb_display);"), 1)
        self.assertLess(terminal.index("drop(ramfb_display);"), terminal.index("persist_and_report_stop!("))
        self.assertNotIn("_ramfb_display", source)

    def test_the_thread_prints_its_stop_record_once_as_it_ends(self):
        source = (PROBE / "ramfb_display_thread.rs").read_text()
        self.assertEqual(source.count('"ramfb display export: stopped '), 1)
        loop = source[source.index("fn export_loop("):]
        self.assertLess(loop.index("while !stop.wait_until(next)"), loop.index('"ramfb display export: stopped '))

    def test_readers_take_the_record_before_the_report_and_refuse_it_after_the_footer(self):
        before = (READERS.AGENT + EXPORT_STOP + READERS.report(SYSTEM_OFF)).encode()
        after = (READERS.AGENT + READERS.report(SYSTEM_OFF, tail=EXPORT_STOP + READERS.TEARDOWN)).encode()
        self.assertIsNotNone(system_off_offset(before))
        self.assertIsNone(system_off_offset(after))
        with tempfile.TemporaryDirectory() as scratch:
            for name, log, expected in (("before", before, 0), ("after", after, 1)):
                path = Path(scratch) / name
                path.write_bytes(log)
                shell = subprocess.run(["/bin/bash", str(READERS.SHELL), "--require-system-off", str(path)],
                                       capture_output=True, timeout=120)
                self.assertEqual(shell.returncode != 0, bool(expected), name)


if __name__ == "__main__":
    result = unittest.main(argv=[sys.argv[0]], exit=False).result
    raise SystemExit(0 if result.wasSuccessful() else 1)
