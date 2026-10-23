#!/usr/bin/env python3
"""The HDA diagnostic records: once each, in print order, before the CoreAudio teardown, in every reader.

As the platform drops the HDA controller after the final report, a controller
whose CoreAudio sink installed host records prints `hda poll:`, `hda stream:`,
`hda repeat:` and `hda CoreAudio timing:` (diagnostics_teardown.rs) just before
the sink prints its lifecycle, stats and continuity records. Every reader of the
post-footer host tail admits each well-formed record at most once, in that order
and before any stats or continuity record, against the unchanged 16-record and
4 KiB bounds, which the widest possible tail fits. A log from before the records
reads as it did and nothing requires them; the B7 lane result, the B7 receipt
and its verifier, T17's audio check and the A5 classifier input are identical
with and without them. HvfHostDiagnosticsTests.swift runs these cases on the
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
CRATE = ROOT / "crates/bridgevm-hvf"
sys.path.insert(0, str(LIVE_GATES))
import hvf_host_diagnostics as diagnostics  # noqa: E402
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
B7 = load("bridgevm_b7_lane_diagnostics", ROOT / "scripts/audio-teardown-result.py")
B7_WRITER = load("bridgevm_b7_writer_diagnostics", LIVE_GATES / "write-audio-teardown-receipt.py")
A5 = load("bridgevm_a5_classifier_diagnostics", ROOT / "scripts/audio-playback-result.py")
POLL = ("hda poll: polls=3 gap_le_10ms=2 gap_le_20ms=0 gap_le_50ms=0 gap_le_100ms=0 gap_gt_100ms=1 max_gap_ms=250 "
        "capped_polls=1 capped_lost_ms=150 ioc_completions=11 max_iocs_per_poll=10 msis_sent=1")
STREAM = ("hda stream: runs=1 rate_hz=48000 channels=2 bits=16 cbl_bytes=3840 cbl_ms=20.000 entries=2 "
          "entry_bytes_min=1920 entry_bytes_max=1920 entry_ms_min=10.000 entry_ms_max=10.000 ioc_entries=2 "
          "position_buffer=1 lpib_reads=7 running_ms=260")
REPEAT = "hda repeat: passes=12 silent_passes=2 compared_passes=8 repeat_passes=3 max_repeat_run=2"
TIMING = ("hda CoreAudio timing: callbacks=10 interval_le_10ms=1 interval_le_15ms=2 interval_le_20ms=1 "
          "interval_le_30ms=2 interval_le_50ms=2 interval_gt_50ms=1 max_interval_ms=51 late_callbacks=3 "
          "trylock_misses=0 ring_samples=4 ring_min_ms=30 ring_max_ms=50 ring_final_ms=30 ring_trimmed_ms=7")
RECORDS = (POLL, STREAM, REPEAT, TIMING)
DIAG = "".join(record + "\n" for record in RECORDS)
CONTINUITY = ("hda CoreAudio continuity: active_callbacks=412 underrun_callbacks=9 underrun_frames=2160 "
              "contention_callbacks=1 gaps=4 max_gap_frames=960 stream_stops=1 callback_frames=480\n")
STATS, TEARDOWN, SHORT = READERS.STATS, READERS.TEARDOWN + CONTINUITY, READERS.SHORT.decode()
LIFECYCLE = READERS.TEARDOWN[:-len(STATS)]
VIRGL = "Sep  1 12:57:02  virgl_render_server[2147483647] <Debug>: socket disconnected\n"
U64_MAX = "18446744073709551615"
MALFORMED = {
    "a missing field": POLL.replace(" msis_sent=1", ""),
    "an extra field": REPEAT + " dropouts=0",
    "reordered fields": REPEAT.replace("passes=12 silent_passes=2", "silent_passes=2 passes=12"),
    "a leading zero": POLL.replace("gap_le_10ms=2", "gap_le_10ms=02"),
    "a 21-digit value": TIMING.replace("callbacks=10", "callbacks=" + "1" * 21),
    "a negative value": TIMING.replace("ring_min_ms=30", "ring_min_ms=-30"),
    "an empty value": REPEAT.replace("passes=12", "passes="),
    "whole milliseconds": STREAM.replace("cbl_ms=20.000", "cbl_ms=20"),
    "two decimals": STREAM.replace("entry_ms_min=10.000", "entry_ms_min=10.00"),
    "a position buffer of 2": STREAM.replace("position_buffer=1", "position_buffer=2"),
    "a trailing space": TIMING + " ",
    "a carriage return": TIMING + "\r",
}
# Host tails B7, T17 and A5 read, and whether every tail reader admits them.
AUDIO = {
    "no records, as every log before them": (TEARDOWN, True),
    "the four records before the teardown records": (DIAG + TEARDOWN, True),
    "the four records before a teardown without continuity": (DIAG + READERS.TEARDOWN, True),
    "the timing record alone": (TIMING + "\n" + TEARDOWN, True),
    "a record between the stats and continuity": (LIFECYCLE + STATS + POLL + "\n" + CONTINUITY, False),
    "a record after the continuity record": (TEARDOWN + POLL + "\n", False),
    "two poll records": (POLL + "\n" + DIAG + TEARDOWN, False),
    "records out of print order": (STREAM + "\n" + POLL + "\n" + TEARDOWN, False),
    **{f"a record with {name}": (line + "\n" + TEARDOWN, False) for name, line in MALFORMED.items()},
}


def widest_value(field: str) -> str:
    return "1" if field == "position_buffer" else U64_MAX + ".999" if field in diagnostics.FIELD_VALUES else U64_MAX


# Every field at its widest, one stopping enqueue failure per AudioQueue buffer,
# failed stop and dispose: 11 records and 3,211 bytes of the 16 and 4,096 allowed.
WIDEST = ("".join(prefix + "".join(f" {field}={widest_value(field)}" for field in fields) + "\n"
                  for prefix, fields in diagnostics.RECORDS.items())
          + "hda CoreAudio callback enqueue: state=stopping reason=stopping-enqueue-during-reset osstatus=-2147483648 "
            "expected=true\n" * 3
          + "".join(f"hda CoreAudio lifecycle: operation={op} osstatus=-2147483648 success=false\n" for op in ("stop", "dispose"))
          + hvf_host_tail.STATS_PREFIX + "".join(f" {field}={U64_MAX}" for field in B7.STAT_FIELDS) + "\n"
          + hvf_host_tail.CONTINUITY_PREFIX + "".join(f" {field}={U64_MAX}" for field in hvf_host_tail.CONTINUITY_FIELDS) + "\n")


def padded(size: int) -> str:
    """A tail of exactly `size` bytes: the four records, then one padded stats record."""
    head = "hda CoreAudio stats: a="
    return DIAG + head + "1" * (size - len(DIAG) - len(head) - 1) + "\n"


BOUNDS = {
    "sixteen records with the four": (DIAG + SHORT * 12, True),
    "seventeen records with the four": (DIAG + SHORT * 13, False),
    "4096 bytes with the four": (padded(4096), True),
    "4097 bytes with the four": (padded(4097), False),
    "the widest tail": (WIDEST, True),
}
# Only the HVF binding admits the render server's line; T17's audio tail refuses it.
VIRGL_TAILS = {
    "the render server's line after the teardown": (DIAG + TEARDOWN + VIRGL, True),
    "the render server's line between the records and the teardown": (DIAG + VIRGL + TEARDOWN, True),
    "the widest tail with the render server's line": (WIDEST + VIRGL, True),
    "the render server's line before a misplaced record": (TEARDOWN + VIRGL + POLL + "\n", False),
}


def audio_log(tail: str) -> str:
    return READERS.AGENT + READERS.report(READERS.SYSTEM_OFF, tail=tail)


def run(*args: str) -> subprocess.CompletedProcess:
    return subprocess.run(list(args), cwd=ROOT, capture_output=True, text=True, timeout=120)


def printed(source: Path, prefix: str) -> str:
    """The format string of the Rust record that starts with `prefix`."""
    [found] = re.findall(r'"(' + re.escape(prefix) + r'(?: [a-z0-9_]+=\{[a-z0-9_]*\})+)"', source.read_text(encoding="utf-8"))
    return found


class DiagnosticsHostTailContract(unittest.TestCase):
    def test_every_reader_names_the_fields_the_host_prints(self):
        sources = {"hda poll:": CRATE / "src/hda/diagnostics.rs", "hda stream:": CRATE / "src/hda/diagnostics.rs",
                   "hda repeat:": CRATE / "src/hda/repeat_detector.rs",
                   "hda CoreAudio timing:": CRATE / "examples/hvf_gic_boot_probe/hda_coreaudio_timing.rs"}
        self.assertEqual(tuple(sources), tuple(diagnostics.RECORDS), "print order")
        for prefix, source in sources.items():
            with self.subTest(prefix):
                text = printed(source, prefix)
                self.assertEqual(tuple(re.findall(r" ([a-z0-9_]+)=\{", text)), diagnostics.RECORDS[prefix])
                widest = re.sub(r" ([a-z0-9_]+)=\{[a-z0-9_]*\}", lambda m: f" {m[1]}={widest_value(m[1])}", text)
                self.assertIsNotNone(diagnostics.GRAMMARS[prefix].fullmatch(widest), "u64::MAX in every field")
        rust = (CRATE / "src/hda/diagnostics.rs").read_text(encoding="utf-8")
        self.assertIn('format!("{}.{:03}", micros / 1000, micros % 1000)', rust, "*_ms buffer lengths keep 3 decimals")
        self.assertIn("u8::from(s.position_buffer)", rust)
        self.assertRegex(rust, r"diagnostics\.poll_record\(\),\s+diagnostics\.stream_record\(\),\s+"
                               r"diagnostics\.repeats\.record\(\),\s+\];\s+if let Some\(host_records\)")
        swift = (ROOT / "apps/macos/Sources/BridgeVMProductE2E/HvfHostDiagnostics.swift").read_text(encoding="utf-8")
        entries = re.findall(r'\("([^"]+:)", \[(.*?)\]\)', swift, re.S)
        self.assertEqual({prefix: tuple(re.findall(r'"([a-z0-9_]+)"', fields)) for prefix, fields in entries},
                         diagnostics.RECORDS)
        self.assertIn(f'static let diagnosticValue = "{diagnostics.VALUE}"', swift)
        self.assertEqual(swift.count('diagnosticValue + #"\\.[0-9]{3}"#'), 3)
        shell = (ROOT / "scripts/hvf-terminal-report.sh").read_text(encoding="utf-8")
        values = dict(re.findall(r" (ms|n)='([^']*)'", shell))
        for prefix, grammar in diagnostics.GRAMMARS.items():
            [pattern] = re.findall(r'-e "(' + re.escape(prefix) + r'[^"]*)"', shell)
            self.assertEqual(pattern.replace("$ms", values["ms"]).replace("$n", values["n"]), grammar.pattern, prefix)
        codes = re.findall(r"s/\^(hda [^/]*?):\.\*/([A-Z])/p", shell)
        self.assertEqual(dict(codes), {hvf_host_tail.STATS_PREFIX[:-1]: "S", hvf_host_tail.CONTINUITY_PREFIX[:-1]: "C",
                                       **{prefix[:-1]: code for prefix, code in diagnostics.DIAGNOSTIC_CODES.items()}})
        self.assertIn(f"=~ ^{diagnostics.DIAGNOSTIC_ORDER}(S+C)?S*$ ]]", shell)

    def test_tail_readers_admit_each_record_once_before_the_teardown(self):
        self.assertEqual((WIDEST.count("\n"), len(WIDEST.encode())), (11, 3248))
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
        nonce, lanes = "a" * 64, {}
        with tempfile.TemporaryDirectory() as temp:
            log, result = Path(temp) / "run.log", Path(temp) / "playback-result.txt"
            result.write_text(f"B7 PLAYBACK PASS nonce={nonce} wav_bytes=384044\n", encoding="utf-8")
            for name, (tail, admitted) in AUDIO.items():
                with self.subTest(name):
                    log.write_text(audio_log(tail), encoding="utf-8")
                    a5 = run("bash", "-c", f'set -euo pipefail; RUN_LOG="$1"; {stats_line}; printf %s "$STATS"', "a5", str(log))
                    stats = run(sys.executable, str(LIVE_GATES / "hvf_terminal_evidence.py"), "--host-stats", str(log))
                    if admitted:
                        lanes[name] = B7.validate(log, result, 0, nonce, 1)
                        guest_evidence._audio(log.read_bytes())
                        self.assertEqual((a5.stdout, stats.stdout), (STATS.rstrip("\n"), STATS))
                        self.assertEqual(A5.validate_result(0, a5.stdout), (96000, 0, 0))
                        continue
                    with self.assertRaises(B7.AudioTeardownError):
                        B7.validate(log, result, 0, nonce, 1)
                    with self.assertRaises(ValueError):
                        guest_evidence._audio(log.read_bytes())
                    self.assertEqual((a5.stdout, stats.returncode), ("", 1))
        old = lanes.pop("no records, as every log before them")
        for name, lane in lanes.items():
            self.assertEqual({k: v for k, v in lane.items() if k != "run_log_sha256"},
                             {k: v for k, v in old.items() if k != "run_log_sha256"}, name)

    def test_b7_lane_cli_receipt_and_verifier_are_identical_with_the_records(self):
        outputs = {}
        with tempfile.TemporaryDirectory() as temp:
            for variant, tail in (("before", TEARDOWN), ("with records", DIAG + TEARDOWN)):
                private = Path(temp) / variant
                for ordinal in range(1, 11):
                    lane, nonce = private / f"lane-{ordinal}", f"{ordinal:064x}"
                    (lane / "share").mkdir(parents=True)
                    (lane / "run.log").write_text(audio_log(tail), encoding="utf-8")
                    (lane / "launcher.exit").write_text("0\n", encoding="ascii")
                    (lane / "nonce").write_text(nonce + "\n", encoding="ascii")
                    (lane / "share" / f"b7-audio-result-{nonce[:12]}.txt").write_text(
                        f"B7 PLAYBACK PASS nonce={nonce} wav_bytes=384044\n", encoding="utf-8")
                lane_json, first = private / "lane-1.json", f"{1:064x}"
                cli = run(sys.executable, "scripts/audio-teardown-result.py", "--run-log", str(private / "lane-1/run.log"),
                          "--result-file", str(private / "lane-1/share" / f"b7-audio-result-{first[:12]}.txt"),
                          "--launcher-exit", "0", "--nonce", first, "--ordinal", "1", "--output", str(lane_json))
                self.assertEqual((cli.returncode, cli.stderr), (0, ""), variant)
                collected, log_set = B7_WRITER.collect(private, 10)
                self.assertEqual(len(collected), 10, variant)
                receipt = B7_WRITER.VERIFY.fixture(True)
                for field in B7.STAT_FIELDS:
                    if f"{field}_total" in receipt:
                        receipt[f"{field}_total"] = sum(lane[field] for lane in collected)
                receipt["run_log_set_sha256"] = log_set
                path = private / "receipt.json"
                path.write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n", encoding="utf-8")
                verified = run(sys.executable, "scripts/verify-audio-teardown-receipt.py", str(path))
                lane_result = json.loads(lane_json.read_text(encoding="utf-8"))
                outputs[variant] = (verified.returncode, verified.stdout, verified.stderr, receipt, lane_result,
                                    [{k: v for k, v in lane.items() if not k.endswith("_sha256")} for lane in collected])
        before, after = outputs["before"], outputs["with records"]
        self.assertEqual(before[:3], (0, "PASS: B7 audio teardown receipt\n", ""))
        self.assertEqual(after[:3], before[:3], "the verifier's output is identical")
        self.assertNotEqual(after[3]["run_log_set_sha256"], before[3]["run_log_set_sha256"], "the logs differ")
        self.assertEqual({**after[3], "run_log_set_sha256": ""}, {**before[3], "run_log_set_sha256": ""})
        self.assertEqual({**after[4], "run_log_sha256": ""}, {**before[4], "run_log_sha256": ""}, "the lane CLI's JSON")
        self.assertEqual(after[5], before[5], "every collected lane")

    def test_t17_terminal_report_admits_the_records_before_the_stats(self):
        for tail, complete in ((DIAG + T17_TAIL.AUDIO_TAIL.decode(), True), (T17_TAIL.AUDIO_TAIL.decode() + DIAG, False),
                               (POLL + "\n" + DIAG + T17_TAIL.AUDIO_TAIL.decode(), False)):
            case = T17_TAIL.fixture.PacketTest(methodName="test_complete_packet_binds_identity_and_keeps_private_bytes_out_of_index")
            case.setUp()
            try:
                case.complete_sources()
                log = case.evidence / "run.log"
                modern = log.read_bytes().replace(b"serial bytes: 23", b"serial raw bytes: 23 output bytes: 23")
                log.write_bytes(modern + tail.encode())
                if complete:
                    T17_TAIL.packet.capture(case.args)
                    T17_TAIL.packet.verify(case.args)
                    self.assertEqual([item["status"] for item in case.index()["artifacts"]], ["retained"] * 4)
                else:
                    with self.assertRaises(T17_TAIL.packet.CaptureError):
                        T17_TAIL.packet.capture(case.args)
            finally:
                case.tearDown()


if __name__ == "__main__":
    unittest.main(verbosity=2)
