#!/usr/bin/env python3
"""The stop binding holds only while its input carries the host's count as a frame.

COUNT_NEW takes at most 8 digits per field. Once the rendered guest tail reaches
10**8 bytes the host's own count is no frame, and a count the guest nests at the
end of its tail becomes the only one, binding the guest's banner and stop line.
A window of run.log can likewise start after the host's count. So the binding
rejects logs of 10**8 bytes or more and its callers pass the whole run.log.
"""

from __future__ import annotations

from pathlib import Path
import sys
import unittest

ROOT = Path(__file__).resolve().parents[2]
SWIFT = ROOT / "apps/macos/Sources/BridgeVMProductE2E"
sys.path.insert(0, str(ROOT / "scripts/live-gates"))
import hvf_terminal_report as binding  # noqa: E402
from hvf_stop_line import SYSTEM_OFF  # noqa: E402
from t17_terminal_report_tail import COUNT_NEW  # noqa: E402

LIMIT = 10**8
BANNER = b"=== EDK2 boot probe (with Apple hv_gic) ==="
OFF, DIAGNOSTIC = SYSTEM_OFF.encode(), b"stop: host diagnostic stop requested"
NESTED = b"\n%s\n%s\nserial raw bytes: 0 output bytes: 0\n--- serial (tail) ---\n" % (BANNER, OFF)


def report(stop: bytes, serial: bytes) -> bytes:
    head = b"BVAGENT READY host=BRIDGEVM t=20075\nREGS: pc=0x0 lr=0x0\n%s\n%s\nexits: 1, last PC: 0x0\n" % (BANNER, stop)
    count = b"serial raw bytes: %d output bytes: %d\n--- serial (tail) ---\n" % (len(serial), len(serial))
    return head + count + serial + b"\n--- end ---\nhda CoreAudio stats: frames_rendered=1 drops=0 callback_errors=0\n"


def sized(stop: bytes, size: int, end: bytes = b"") -> bytes:
    """A report of exactly `size` bytes whose guest tail ends with `end`."""
    tail = size - len(report(stop, b""))
    tail -= 2 * (len(str(tail)) - 1)  # both count fields widen with the tail
    raw = report(stop, b"A" * (tail - len(end)) + end)
    assert len(raw) == size
    return raw


class TerminalStopLimitContract(unittest.TestCase):
    def test_host_count_frames_every_tail_below_the_limit(self):
        self.assertEqual(binding.LOG_LIMIT, LIMIT)
        top = b"%d" % (LIMIT - 1)
        self.assertIsNotNone(COUNT_NEW.fullmatch(b"serial raw bytes: " + top + b" output bytes: " + top))
        source = (SWIFT / "HvfTerminalReport.swift").read_text(encoding="utf-8")
        for line in (f"static let logLimit = {LIMIT:_}\n", "guard log.count < logLimit, let tailEnd = tailEnd(log) else"):
            self.assertTrue(line in source, line)

    def test_guest_frame_ending_a_long_tail_binds_nothing(self):
        for size in (LIMIT - 1, LIMIT, LIMIT + 1024):
            raw = sized(DIAGNOSTIC, size, NESTED)
            self.assertIsNone(binding.terminal_stop(raw), size)
            self.assertIsNone(binding.system_off_offset(raw), size)
        raw = report(DIAGNOSTIC, b"A" * LIMIT + NESTED)
        self.assertIsNone(binding.terminal_stop(raw))

    def test_honest_report_binds_only_below_the_limit(self):
        raw = sized(OFF, LIMIT - 1, b"\r\n")
        self.assertEqual(binding.terminal_stop(raw), (raw.index(b"\n%s\n" % OFF) + 1, SYSTEM_OFF))
        self.assertIsNone(binding.terminal_stop(sized(OFF, LIMIT, b"\r\n")))

    def test_stop_waits_pass_the_whole_run_log(self):
        swift = {name: (SWIFT / name).read_text(encoding="utf-8")
                 for name in ("T17ProductRunner.swift", "A9ImportProductRunner.swift", "T17GuestJourney.swift", "T17BoundedLog.swift")}
        journey = swift["T17GuestJourney.swift"]
        for line in ("guard let data = try? Data(contentsOf: self.runLog), data.count > before, let stop = HvfTerminalReport.stop(in: data) else { return false }\n"
                     "            return stop.offset >= before && stop.line == HvfStopLine.systemOff\n",
                     "}), waitForStableLog() else {\n"):
            self.assertEqual(journey.count(line), 1, line)
        self.assertFalse("systemOffObserved" in journey or "contains(HvfStopLine.systemOff)" in journey)
        for name, log, reader in (("T17ProductRunner.swift", "T17BoundedLog.text(log, whole: true)", "T17BoundedLog.swift"),
                                  ("A9ImportProductRunner.swift", "self.boundedText(self.runLog, whole: true)", "A9ImportProductRunner.swift")):
            self.assertEqual(swift[name].count("HvfStopLine.systemOffObserved(in:"), 1, name)
            self.assertTrue(f"HvfStopLine.systemOffObserved(in: {log})" in swift[name], name)
            for line in ('guard !whole || size <= 64 * 1024 * 1024 else { return "" }',
                         "seek(toOffset: !whole && size > 8 * 1024 * 1024 ? size - 8 * 1024 * 1024 : 0)"):
                self.assertTrue(line in swift[reader], f"{reader}: {line}")


if __name__ == "__main__":
    unittest.main(verbosity=2)
