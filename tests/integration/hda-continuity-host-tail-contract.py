#!/usr/bin/env python3
"""The CoreAudio continuity record: at most one per host tail, after the stats, in every reader.

hda_coreaudio_continuity.rs prints `hda CoreAudio continuity:` after the stats
record whose exact field set B7 and T17 parse. Every reader of the final
report's post-footer host tail admits at most one well-formed record there,
after a stats record, counted against the unchanged 16-record and 4 KiB
bounds. A log from before the record reads as it did and nothing requires
one; the stats record, the B7 receipt and the A5 classifier input are
unchanged. hda_continuity.py reports the record and refuses one whose
counters do not reconcile. HvfHostTailTests.swift runs these cases on the
Swift twins.
"""

from __future__ import annotations

import importlib.util
import json
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
LIVE_GATES = ROOT / "scripts/live-gates"
PROBE = ROOT / "crates/bridgevm-hvf/examples/hvf_gic_boot_probe"
sys.path.insert(0, str(LIVE_GATES))
import hda_continuity  # noqa: E402
import hvf_host_tail  # noqa: E402
import windows_product_e2e_guest_evidence as guest_evidence  # noqa: E402
from hvf_terminal_report import system_off_offset  # noqa: E402


def load(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


READERS = load("hvf_stop_readers_contract", ROOT / "tests/integration/hvf-stop-readers-contract.py")
T17_TAIL = load("t17_terminal_report_tail_test", ROOT / "tests/integration/t17-terminal-report-tail-test.py")
B7 = load("bridgevm_b7_lane_continuity", ROOT / "scripts/audio-teardown-result.py")
A5 = load("bridgevm_a5_classifier_continuity", ROOT / "scripts/audio-playback-result.py")
CONTINUITY = ("hda CoreAudio continuity: active_callbacks=412 underrun_callbacks=9 underrun_frames=2160 "
              "contention_callbacks=1 gaps=4 max_gap_frames=960 stream_stops=1 callback_frames=480")
SILENT = CONTINUITY.split(" active_callbacks=")[0] + "".join(
    f" {field}={480 if field == 'callback_frames' else 0}" for field in hvf_host_tail.CONTINUITY_FIELDS)
STATS, TEARDOWN, SHORT = READERS.STATS, READERS.TEARDOWN, READERS.SHORT.decode()
LIFECYCLE = TEARDOWN[:-len(STATS)]
VIRGL = "Sep  1 12:57:02  virgl_render_server[43160] <Debug>: socket disconnected\n"
MALFORMED = {
    "a missing field": CONTINUITY.replace(" stream_stops=1", ""),
    "an extra field": CONTINUITY + " dropouts=0",
    "reordered fields": CONTINUITY.replace("gaps=4 max_gap_frames=960", "max_gap_frames=960 gaps=4"),
    "a leading zero": CONTINUITY.replace("gaps=4", "gaps=04"),
    "a 21-digit value": CONTINUITY.replace("active_callbacks=412", "active_callbacks=" + "1" * 21),
    "a negative value": CONTINUITY.replace("gaps=4", "gaps=-4"),
    "an empty value": CONTINUITY.replace("gaps=4", "gaps="),
    "a trailing space": CONTINUITY + " ",
    "a carriage return": CONTINUITY + "\r",
}
# Audio-shaped host tails B7, T17 and A5 read, and whether every tail reader admits them.
AUDIO = {
    "no record, as every log before it": (TEARDOWN, True),
    "one record after the stats": (TEARDOWN + CONTINUITY + "\n", True),
    "a silent record after the stats": (TEARDOWN + SILENT + "\n", True),
    "a record before the stats": (LIFECYCLE + CONTINUITY + "\n" + STATS, False),
    "a record without stats": (LIFECYCLE + CONTINUITY + "\n", False),
    "two records": (TEARDOWN + (CONTINUITY + "\n") * 2, False),
    **{f"a record with {name}": (TEARDOWN + line + "\n", False) for name, line in MALFORMED.items()},
}


def padded(size: int) -> str:
    """A tail of exactly `size` bytes: one padded stats record, then the continuity record."""
    head = "hda CoreAudio stats: a="
    return head + "1" * (size - len(head) - len(CONTINUITY) - 2) + "\n" + CONTINUITY + "\n"


# The continuity record counts against the unchanged record and byte bounds.
BOUNDS = {
    "sixteen records with the continuity record": (SHORT * 15 + CONTINUITY + "\n", True),
    "seventeen records with the continuity record": (SHORT * 16 + CONTINUITY + "\n", False),
    "4096 bytes with the continuity record": (padded(4096), True),
    "4097 bytes with the continuity record": (padded(4097), False),
}
# Only the HVF binding admits the render server's line; T17's audio tail refuses it.
VIRGL_TAILS = {
    "the render server's line after the record": (TEARDOWN + CONTINUITY + "\n" + VIRGL, True),
    "the render server's line between stats and record": (TEARDOWN + VIRGL + CONTINUITY + "\n", True),
    "the render server's line before two records": (TEARDOWN + VIRGL + (CONTINUITY + "\n") * 2, False),
}


def audio_log(tail: str) -> str:
    return READERS.AGENT + READERS.report(READERS.SYSTEM_OFF, tail=tail)


def run(*args: str) -> subprocess.CompletedProcess:
    return subprocess.run(list(args), cwd=ROOT, capture_output=True, text=True, timeout=120)


class ContinuityHostTailContract(unittest.TestCase):
    def test_every_reader_names_the_fields_the_host_prints(self):
        rust = (PROBE / "hda_coreaudio_continuity.rs").read_text(encoding="utf-8")
        self.assertIn(f'const PREFIX: &str = "{hvf_host_tail.CONTINUITY_PREFIX}";', rust)
        printed = re.search(r'"\{PREFIX\}((?: [a-z_]+=\{\})+)"', rust)
        self.assertEqual(tuple(re.findall(r" ([a-z_]+)=\{\}", printed[1])), hvf_host_tail.CONTINUITY_FIELDS)
        widest = hvf_host_tail.CONTINUITY_PREFIX + printed[1].replace("{}", "18446744073709551615")
        self.assertIsNotNone(hvf_host_tail.CONTINUITY.fullmatch(widest), "u64::MAX in every field")
        sink = [line.strip() for line in (PROBE / "hda_coreaudio.rs").read_text(encoding="utf-8").splitlines()]
        stats = sink.index("self.shared.print_stats([stop_status, dispose_status]);")
        self.assertEqual(sink[stats + 1], 'println!("{}", self.shared.continuity.snapshot().record());')
        teardown = (PROBE / "hda_coreaudio_teardown.rs").read_text(encoding="utf-8")
        stat_fields = re.search(r'"hda CoreAudio stats: ([^"]*)"', teardown)[1]
        self.assertEqual(tuple(re.findall(r"([a-z_]+)=\{", stat_fields)), B7.STAT_FIELDS, "the stats record is unchanged")
        swift = (ROOT / "apps/macos/Sources/BridgeVMProductE2E/HvfHostTail.swift").read_text(encoding="utf-8")
        fields = re.search(r"static let continuityFields = \[(.*?)\]", swift, re.S)[1]
        self.assertEqual(tuple(re.findall(r'"([a-z_]+)"', fields)), hvf_host_tail.CONTINUITY_FIELDS)
        self.assertIn('" \\($0)=(0|[1-9][0-9]{0,19})"', swift)
        shell = (ROOT / "scripts/hvf-terminal-report.sh").read_text(encoding="utf-8")
        value = re.search(r" n='([^']*)'\n", shell)[1]
        [pattern] = re.findall(r'-e "(hda CoreAudio continuity:[^"]*)"', shell)
        self.assertEqual(pattern.replace("$n", value), hvf_host_tail.CONTINUITY.pattern)

    def test_tail_readers_admit_one_record_after_the_stats(self):
        cases = {**AUDIO, **BOUNDS, **VIRGL_TAILS}
        with tempfile.TemporaryDirectory() as temp:
            log = Path(temp) / "run.log"
            for name, (tail, admitted) in cases.items():
                with self.subTest(name):
                    data = READERS.honest(tail=tail.encode())
                    log.write_bytes(data)
                    self.assertEqual(system_off_offset(data) is not None, admitted)
                    expected = 0 if admitted else 1
                    shell = run("/bin/bash", "scripts/hvf-terminal-report.sh", "--require-system-off", str(log))
                    cli = run(sys.executable, str(LIVE_GATES / "hvf_terminal_evidence.py"), "--require-system-off", str(log))
                    self.assertEqual((shell.returncode, cli.returncode), (expected, expected), shell.stderr + cli.stderr)
                    t17 = admitted and name not in VIRGL_TAILS
                    self.assertEqual(hvf_host_tail.bounded_host_shutdown_tail(tail), t17)

    def test_b7_t17_and_a5_read_the_unchanged_stats_record(self):
        verifier = ROOT / "scripts/verify-audio-playback.sh"
        [stats_line] = [line for line in verifier.read_text(encoding="utf-8").splitlines() if line.startswith("STATS=")]
        nonce, receipts = "a" * 64, {}
        with tempfile.TemporaryDirectory() as temp:
            log, result = Path(temp) / "run.log", Path(temp) / "playback-result.txt"
            result.write_text(f"B7 PLAYBACK PASS nonce={nonce} wav_bytes=384044\n", encoding="utf-8")
            for name, (tail, admitted) in AUDIO.items():
                with self.subTest(name):
                    log.write_text(audio_log(tail), encoding="utf-8")
                    a5 = run("bash", "-c", f'set -euo pipefail; RUN_LOG="$1"; {stats_line}; printf %s "$STATS"', "a5", str(log))
                    if admitted:
                        receipts[name] = B7.validate(log, result, 0, nonce, 1)
                        guest_evidence._audio(log.read_bytes())
                        self.assertEqual(a5.stdout, STATS.rstrip("\n"))
                        self.assertEqual(A5.validate_result(0, a5.stdout), (96000, 0, 0))
                        continue
                    with self.assertRaises(B7.AudioTeardownError):
                        B7.validate(log, result, 0, nonce, 1)
                    with self.assertRaises(ValueError):
                        guest_evidence._audio(log.read_bytes())
                    self.assertEqual(a5.stdout, "")
        old = receipts.pop("no record, as every log before it")
        for name, receipt in receipts.items():
            self.assertEqual(receipt.keys(), old.keys(), f"{name}: the B7 receipt format is unchanged")
            self.assertEqual({k: v for k, v in receipt.items() if not k.endswith("_sha256")},
                             {k: v for k, v in old.items() if not k.endswith("_sha256")}, name)

    def test_t17_terminal_report_admits_one_record_after_the_stats(self):
        for tail, complete in ((CONTINUITY + "\n", True), ((CONTINUITY + "\n") * 2, False), (MALFORMED["a leading zero"] + "\n", False)):
            case = T17_TAIL.fixture.PacketTest(methodName="test_complete_packet_binds_identity_and_keeps_private_bytes_out_of_index")
            case.setUp()
            try:
                case.complete_sources()
                log = case.evidence / "run.log"
                modern = log.read_bytes().replace(b"serial bytes: 23", b"serial raw bytes: 23 output bytes: 23")
                log.write_bytes(modern + T17_TAIL.AUDIO_TAIL + tail.encode())
                if complete:
                    T17_TAIL.packet.capture(case.args)
                    T17_TAIL.packet.verify(case.args)
                    self.assertEqual([item["status"] for item in case.index()["artifacts"]], ["retained"] * 4)
                else:
                    with self.assertRaises(T17_TAIL.packet.CaptureError):
                        T17_TAIL.packet.capture(case.args)
            finally:
                case.tearDown()

    def test_helper_reports_only_the_host_record(self):
        record = audio_log(TEARDOWN + CONTINUITY + "\n").encode()
        values = hda_continuity.continuity(record)
        self.assertEqual(tuple(values), hvf_host_tail.CONTINUITY_FIELDS)
        self.assertEqual(hda_continuity.record(values), CONTINUITY)
        self.assertEqual(hda_continuity.continuity(audio_log(TEARDOWN + SILENT + "\n").encode())["gaps"], 0)
        for raw, reason in ((audio_log(TEARDOWN).encode(), "no continuity record"),
                            (READERS.honest(serial=f"boot\r\n{CONTINUITY}\r\n".encode()), "no continuity record"),
                            (READERS.AGENT.replace("bridgevm\\user", CONTINUITY).encode()
                             + READERS.honest(tail=b"")[len(READERS.AGENT):], "no continuity record"),
                            ((READERS.AGENT + TEARDOWN + CONTINUITY + "\n").encode(), "no final host report"),
                            (audio_log(TEARDOWN + (CONTINUITY + "\n") * 2).encode(), "no final host report")):
            with self.assertRaisesRegex(hda_continuity.ContinuityError, reason):
                hda_continuity.continuity(raw)
        helper = str(LIVE_GATES / "hda_continuity.py")
        with tempfile.TemporaryDirectory() as temp:
            log, old, link = Path(temp) / "run.log", Path(temp) / "old.log", Path(temp) / "link.log"
            log.write_bytes(record)
            old.write_text(audio_log(TEARDOWN), encoding="utf-8")
            link.symlink_to(log)
            reported = run(sys.executable, helper, str(log))
            self.assertEqual((reported.stdout, reported.returncode), (CONTINUITY + "\n", 0))
            self.assertEqual(list(json.loads(run(sys.executable, helper, "--json", str(log)).stdout)),
                             list(hvf_host_tail.CONTINUITY_FIELDS))
            absent = run(sys.executable, helper, str(old))
            self.assertEqual(absent.returncode, 1)
            self.assertIn("no continuity record", absent.stderr)
            self.assertEqual(run(sys.executable, helper, str(link)).returncode, 1, "a linked run.log reads as empty")
            for usage in ((), ("--json",), ("--csv", str(log)), (str(log), str(old))):
                self.assertEqual(run(sys.executable, helper, *usage).returncode, 2, usage)

    def test_helper_refuses_counters_that_do_not_reconcile(self):
        self.assertEqual(hda_continuity.parse(SILENT)["active_callbacks"], 0)
        for old, new, reason in (
                ("callback_frames=480", "callback_frames=0", "callback_frames is zero"),
                ("contention_callbacks=1", "contention_callbacks=10", "do not nest"),
                ("active_callbacks=412", "active_callbacks=8", "do not nest"),
                ("gaps=4", "gaps=10", "each gap needs"),
                ("gaps=4", "gaps=0", "not all zero or all nonzero"),
                ("underrun_callbacks=9", "underrun_callbacks=4", "do not fit"),
                ("contention_callbacks=1", "contention_callbacks=5", "do not fit"),
                ("max_gap_frames=960", "max_gap_frames=2161", "longest gap"),
                ("max_gap_frames=960", "max_gap_frames=500", "longest gap")):
            with self.subTest(new), self.assertRaisesRegex(hda_continuity.ContinuityError, reason):
                hda_continuity.parse(CONTINUITY.replace(old, new, 1))
        for name, line in MALFORMED.items():
            with self.subTest(name), self.assertRaisesRegex(hda_continuity.ContinuityError, "malformed"):
                hda_continuity.parse(line)


if __name__ == "__main__":
    unittest.main(verbosity=2)
