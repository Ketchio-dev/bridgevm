#!/usr/bin/env python3
"""The ramfb display timing record: at most once, after every HDA record, in every post-footer tail reader.

As the platform drops after the final report it prints a running ramfb display
exporter's `ramfb display timing:` record (teardown_records.rs,
ramfb_display_timing.rs) after the HDA controller's records and the CoreAudio
teardown records. Every reader of the post-footer host tail (the Python twin,
the packaged shell twin, the evidence CLI and T17's bounded audio tail) admits
one well-formed record there, against the unchanged 16-record and 4 KiB bounds
that the widest tail with it still fits, and refuses it anywhere else, twice or
malformed. The HDA readers read the same values with it, and every log from
before it reads as it did: hda-diagnostics-host-tail-contract.py and the other
tail contracts run unchanged. HvfHostDisplayTests.swift runs these cases on the
Swift twins.
"""

from __future__ import annotations

import importlib.util
from pathlib import Path
import re
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
LIVE_GATES = ROOT / "scripts/live-gates"
PROBE = ROOT / "crates/bridgevm-hvf/examples/hvf_gic_boot_probe"
sys.path.insert(0, str(LIVE_GATES))
import hvf_host_display as display  # noqa: E402
import hvf_host_tail  # noqa: E402
from hvf_terminal_report import system_off_offset  # noqa: E402

SPEC = importlib.util.spec_from_file_location(
    "hda_diagnostics_host_tail_contract", ROOT / "tests/integration/hda-diagnostics-host-tail-contract.py")
assert SPEC and SPEC.loader
HDA = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(HDA)

DISPLAY = ("ramfb display timing: ticks=4960 late_le_1ms=4100 late_le_4ms=700 late_le_8ms=120 late_le_16ms=30 "
           "late_gt_16ms=10 max_late_us=41250 published=3202 interval_le_20ms=0 interval_le_40ms=2900 "
           "interval_le_70ms=250 interval_gt_70ms=51 max_interval_ms=9050")
LINE = DISPLAY + "\n"
WIDEST = display.DISPLAY_PREFIX + "".join(f" {field}={HDA.U64_MAX}" for field in display.DISPLAY_FIELDS) + "\n"
# The export thread's own stop record, printed before the final report, byte for byte.
STOP = ('"ramfb display export: stopped polls={polls} published={published} \\\n'
        '         seconds={seconds:.1} poll_hz={poll_hz:.1} publish_hz={publish_hz:.1} \\\n'
        '         slowest_tick_us={}",\n')
MALFORMED = {
    "a missing field": DISPLAY.replace(" published=3202", ""),
    "an extra field": DISPLAY + " dropped=0",
    "reordered fields": DISPLAY.replace("ticks=4960 late_le_1ms=4100", "late_le_1ms=4100 ticks=4960"),
    "a leading zero": DISPLAY.replace("ticks=4960", "ticks=04960"),
    "a 21-digit value": DISPLAY.replace("ticks=4960", "ticks=" + "1" * 21),
    "a negative value": DISPLAY.replace("max_late_us=41250", "max_late_us=-41250"),
    "an empty value": DISPLAY.replace("published=3202", "published="),
    "a decimal value": DISPLAY.replace("max_late_us=41250", "max_late_us=41.250"),
    "a trailing space": DISPLAY + " ",
    "a carriage return": DISPLAY + "\r",
    "the export stop record's prefix": DISPLAY.replace("ramfb display timing:", "ramfb display export:"),
}
TAILS = {
    "no display record, as every log before it": (HDA.DIAG + HDA.TEARDOWN, True),
    "the display record after the HDA teardown": (HDA.DIAG + HDA.TEARDOWN + LINE, True),
    "the display record after a teardown without continuity": (HDA.DIAG + HDA.READERS.TEARDOWN + LINE, True),
    "the display record after the four records": (HDA.DIAG + LINE, True),
    "the display record alone": (LINE, True),
    "the display record before the HDA records": (LINE + HDA.DIAG + HDA.TEARDOWN, False),
    "the display record between the stats and continuity": (HDA.LIFECYCLE + HDA.STATS + LINE + HDA.CONTINUITY, False),
    "an HDA record after the display record": (HDA.TEARDOWN + LINE + HDA.POLL + "\n", False),
    "a stats record after the display record": (HDA.DIAG + LINE + HDA.STATS, False),
    "two display records": (HDA.DIAG + HDA.TEARDOWN + LINE + LINE, False),
    **{f"a display record with {name}": (HDA.TEARDOWN + line + "\n", False) for name, line in MALFORMED.items()},
}


def padded(size: int) -> str:
    """Exactly `size` bytes: the four records, one padded stats record, then the display record."""
    head = "hda CoreAudio stats: a="
    return HDA.DIAG + head + "1" * (size - len(HDA.DIAG) - len(head) - 1 - len(LINE)) + "\n" + LINE


BOUNDS = {
    "sixteen records with the display record": (HDA.DIAG + HDA.SHORT * 11 + LINE, True),
    "seventeen records with the display record": (HDA.DIAG + HDA.SHORT * 12 + LINE, False),
    "4096 bytes with the display record": (padded(4096), True),
    "4097 bytes with the display record": (padded(4097), False),
    "the widest tail with the widest display record": (HDA.WIDEST + WIDEST, True),
}
# Only the HVF binding admits the render server's line; T17's audio tail refuses it.
VIRGL_TAILS = {
    "the render server's line after the display record": (HDA.DIAG + HDA.TEARDOWN + LINE + HDA.VIRGL, True),
    "the widest tail with both": (HDA.WIDEST + WIDEST + HDA.VIRGL, True),
}


class DisplayHostTailContract(unittest.TestCase):
    def test_every_reader_names_the_fields_the_probe_prints(self):
        text = HDA.printed(PROBE / "ramfb_display_timing.rs", display.DISPLAY_PREFIX)
        self.assertEqual(tuple(re.findall(r" ([a-z0-9_]+)=\{", text)), display.DISPLAY_FIELDS)
        swift = (ROOT / "apps/macos/Sources/BridgeVMProductE2E/HvfHostDisplay.swift").read_text(encoding="utf-8")
        [(prefix, fields)] = re.findall(r'\("([^"]+:)", \[(.*?)\]\)', swift, re.S)
        self.assertEqual((prefix, tuple(re.findall(r'"([a-z0-9_]+)"', fields))),
                         (display.DISPLAY_PREFIX, display.DISPLAY_FIELDS))
        shell = (ROOT / "scripts/hvf-terminal-report.sh").read_text(encoding="utf-8")
        values = dict(re.findall(r" (ms|n)='([^']*)'", shell))
        [pattern] = re.findall(r'-e "(' + re.escape(display.DISPLAY_PREFIX) + r'[^"]*)"', shell)
        self.assertEqual((values["n"], pattern.replace("$n", values["n"])), (display.VALUE, display.DISPLAY_LINE.pattern))
        thread = (PROBE / "ramfb_display_thread.rs").read_text(encoding="utf-8")
        self.assertEqual(thread.count(STOP), 1, "the export's stop record is byte-identical")

    def test_tail_readers_admit_one_record_after_every_hda_record(self):
        widest = HDA.WIDEST + WIDEST
        self.assertEqual((widest.count("\n"), len(widest.encode())), (12, 3717))
        with tempfile.TemporaryDirectory() as temp:
            log = Path(temp) / "run.log"
            for name, (tail, admitted) in {**TAILS, **BOUNDS, **VIRGL_TAILS}.items():
                with self.subTest(name):
                    data = HDA.READERS.honest(tail=tail.encode())
                    log.write_bytes(data)
                    self.assertEqual(system_off_offset(data) is not None, admitted)
                    expected = 0 if admitted else 1
                    shell = HDA.run("/bin/bash", "scripts/hvf-terminal-report.sh", "--require-system-off", str(log))
                    cli = HDA.run(sys.executable, str(LIVE_GATES / "hvf_terminal_evidence.py"), "--require-system-off",
                                  str(log))
                    self.assertEqual((shell.returncode, cli.returncode), (expected, expected), shell.stderr + cli.stderr)
                    t17 = admitted and name not in VIRGL_TAILS
                    self.assertEqual(hvf_host_tail.bounded_host_shutdown_tail(tail), t17)

    def test_hda_readers_read_the_same_records_with_the_display_record(self):
        with tempfile.TemporaryDirectory() as temp:
            outputs = []
            for tail in (HDA.DIAG + HDA.TEARDOWN, HDA.DIAG + HDA.TEARDOWN + LINE):
                log = Path(temp) / f"run-{len(outputs)}.log"
                log.write_bytes(HDA.READERS.honest(tail=tail.encode()))
                runs = [HDA.run(sys.executable, str(LIVE_GATES / script), *args, str(log))
                        for script, args in (("hda_diagnostics.py", ["--json"]), ("hda_continuity.py", ["--json"]),
                                             ("hvf_terminal_evidence.py", ["--host-stats"]))]
                outputs.append([(run.returncode, run.stdout, run.stderr) for run in runs])
        self.assertEqual([code for code, _, _ in outputs[0]], [0, 0, 0])
        self.assertEqual(outputs[1], outputs[0])

    def test_t17_terminal_report_admits_the_record_after_the_audio_tail(self):
        audio = HDA.DIAG + HDA.T17_TAIL.AUDIO_TAIL.decode()
        for tail, complete in ((audio + LINE, True), (LINE + audio, False), (audio + LINE + LINE, False)):
            case = HDA.T17_TAIL.fixture.PacketTest(
                methodName="test_complete_packet_binds_identity_and_keeps_private_bytes_out_of_index")
            case.setUp()
            try:
                case.complete_sources()
                log = case.evidence / "run.log"
                modern = log.read_bytes().replace(b"serial bytes: 23", b"serial raw bytes: 23 output bytes: 23")
                log.write_bytes(modern + tail.encode())
                if complete:
                    HDA.T17_TAIL.packet.capture(case.args)
                    HDA.T17_TAIL.packet.verify(case.args)
                else:
                    with self.assertRaises(HDA.T17_TAIL.packet.CaptureError):
                        HDA.T17_TAIL.packet.capture(case.args)
            finally:
                case.tearDown()


if __name__ == "__main__":
    unittest.main(verbosity=2)
