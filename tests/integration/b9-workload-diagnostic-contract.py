#!/usr/bin/env python3
"""Synthetic B9 CSV contracts; these fixtures are never workload evidence."""

import hashlib
import json
from pathlib import Path
import runpy
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
from b9_workload_diagnostic import (DECLARATION, DiagnosticError, EXPECTED_DECLARATION,
                                    MAX_CSV_BYTES, load_declaration, summarize_csv)

HEADER = "Application,ProcessID,SwapChainAddress,CPUStartTime,FrameTime,CPUBusy,CPUWait,PresentMode\r\n"


def rows(times, *, app="vlc.exe", pid="123", swap="0x20"):
    start = 0.0
    result = []
    for frame in times:
        result.append(f"{app},{pid},{swap},{start:.4f},{frame:.4f},{frame:.4f},0,Composed\r\n")
        start += frame
    return result


class CandidateCsvContracts(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)

    def evidence(self, text=None, *, name="capture.csv", bom=True):
        if text is None:
            text = HEADER + "".join(rows([10, 11, 12]))
        raw = ("\ufeff" + text if bom else text).encode("utf-8")
        path = self.root / name
        path.write_bytes(raw)
        return path, hashlib.sha256(raw).hexdigest()

    def assert_rejected(self, text):
        with self.assertRaises(DiagnosticError):
            summarize_csv(*self.evidence(text), 123)

    def test_fixed_candidate_and_existing_presentmon_pin(self):
        declaration, digest = load_declaration()
        self.assertEqual(declaration, EXPECTED_DECLARATION)
        self.assertEqual(digest, hashlib.sha256(DECLARATION.read_bytes()).hexdigest())
        self.assertTrue(declaration["candidate_only"])
        self.assertEqual(declaration["required_workloads"], 20)
        self.assertEqual(declaration["application"]["sha256"],
                         "9c0917dc521ffc8ce30e70bca7f6c9dc8fec80909d763e75cd976351dee8db0b")
        self.assertEqual(declaration["media"]["sha256"],
                         "5e43740e2916afc1b17de09f4948f038b83065a5d42f0fcb16643d7faeb00ad7")
        namespace = runpy.run_path(str(ROOT / "scripts/verify-presentmon-binary.py"))
        pin = namespace["PINNED"][declaration["collector"]["asset"]]
        self.assertEqual((declaration["collector"]["sha256"], declaration["collector"]["bytes"]),
                         (pin["sha256"], pin["bytes"]))

    def test_declaration_drift_duplicates_and_symlink_are_rejected(self):
        altered = self.root / "candidate.json"
        for key, replacement in (("required_workloads", 1), ("required_workloads", 20.0), ("candidate_only", 1)):
            changed = json.loads(DECLARATION.read_text()); changed[key] = replacement
            altered.write_text(json.dumps(changed))
            with self.assertRaises(DiagnosticError): load_declaration(altered)
        altered.write_text('{"candidate_only":true,"candidate_only":false}')
        with self.assertRaisesRegex(DiagnosticError, "duplicate"): load_declaration(altered)
        altered.unlink()
        altered.symlink_to(DECLARATION)
        with self.assertRaises(DiagnosticError): load_declaration(altered)

    def test_actual_presentmon_v2_units_nearest_ranks_and_no_claim(self):
        text = HEADER + "".join(rows(list(range(1, 101))))
        report = summarize_csv(*self.evidence(text), 123)
        self.assertEqual((report["frame_count"], report["unit"], report["metric"]),
                         (100, "milliseconds", "FrameTime"))
        self.assertEqual((report["p50_nearest_rank_ms"], report["p95_nearest_rank_ms"],
                          report["p99_nearest_rank_ms"]), (50, 95, 99))
        self.assertEqual(report["cpu_start_span_ms"], 4950)
        self.assertEqual(report["diagnostic_class"], "CSV_APP_ROWS_VALID")
        self.assertEqual((report["target_pid"], report["application"],
                          report["swapchain_address"]), (123, "vlc.exe", "0x20"))
        for name in ("source_assets_verified", "collector_completion_verified",
                     "playback_verified", "display_verified", "claim_eligible",
                     "criterion_pass", "capability_promotion"):
            self.assertFalse(report[name], name)

    def test_utf8_bom_crlf_extra_columns_and_exporter_rounding(self):
        text = HEADER + "vlc.exe,123,0X20,0,10.0001,5.0000,5.0000,Composed\r\n" + \
               "vlc.exe,123,0x20,10.0001,11.0000,11.0000,0,Composed\r\n"
        report = summarize_csv(*self.evidence(text), 123)
        self.assertEqual(report["frame_count"], 2)
        self.assertEqual(report["p95_nearest_rank_ms"], 11)

    def test_wrong_or_missing_caller_hash_and_pid_fail(self):
        path, digest = self.evidence()
        for bad_hash in ("0" * 64, digest.upper(), "", "x" * 64):
            with self.assertRaises(DiagnosticError): summarize_csv(path, bad_hash, 123)
        for bad_pid in (0, -1, True, 124, 2**32):
            with self.assertRaises(DiagnosticError): summarize_csv(path, digest, bad_pid)

    def test_other_process_dwm_and_multiple_swapchains_fail_closed(self):
        first = rows([10])[0]
        for second in (rows([11], app="dwm.exe")[0], rows([11], app="VLC.exe")[0],
                       rows([11], pid="124")[0], rows([11], swap="0x21")[0],
                       rows([11], swap="0x0")[0], rows([11], swap="not-hex")[0],
                       rows([11], swap="0x10000000000000000")[0]):
            # Keep a contiguous time axis; the identity must still fail first.
            second = second.replace(",0.0000,", ",10.0000,")
            self.assert_rejected(HEADER + first + second)

    def test_missing_duplicate_ragged_multiline_and_blank_rows_fail(self):
        normal = "".join(rows([10, 11]))
        for header in (HEADER.replace("FrameTime", "GPUTime"),
                       HEADER.replace("CPUWait", "FrameTime")):
            self.assert_rejected(header + normal)
        self.assert_rejected(HEADER + normal.replace(",Composed\r\n", "\r\n", 1))
        self.assert_rejected(HEADER + normal.replace(",Composed\r\n", ",Composed,extra\r\n", 1))
        self.assert_rejected(HEADER + rows([10])[0] + "\r\n" +
                             rows([11])[0].replace(",0.0000,", ",10.0000,"))
        self.assert_rejected(HEADER + rows([10])[0].replace("Composed", '"Com\r\nposed"') +
                             rows([11])[0].replace(",0.0000,", ",10.0000,"))

    def test_nonfinite_nonpositive_and_busy_wait_mismatch_fail(self):
        normal = HEADER + "".join(rows([10, 11]))
        for value in ("0", "-1", "NaN", "inf", "-inf", "NA", "1e309", "1_0", " 10"):
            self.assert_rejected(normal.replace(",10.0000,10.0000,0,", f",{value},10.0000,0,", 1))
        self.assert_rejected(normal.replace(",10.0000,10.0000,0,", ",10.0000,1,1,", 1))
        self.assert_rejected(normal.replace(",0.0000,10.0000,", ",-1,10.0000,", 1))

    def test_timeline_gaps_duplicates_reordering_and_low_count_fail(self):
        first = rows([10])[0]
        for start in ("0", "9", "20", "NaN", "inf"):
            second = rows([11])[0].replace(",0.0000,", f",{start},")
            self.assert_rejected(HEADER + first + second)
        self.assert_rejected(HEADER)
        self.assert_rejected(HEADER + first)

    def test_file_kind_size_utf8_and_nul_fail(self):
        path, digest = self.evidence()
        link = self.root / "alias.csv"
        link.symlink_to(path)
        with self.assertRaises(DiagnosticError): summarize_csv(link, digest, 123)
        with path.open("wb") as stream: stream.truncate(MAX_CSV_BYTES + 1)
        with self.assertRaises(DiagnosticError): summarize_csv(path, digest, 123)
        bad = self.root / "bad.csv"
        bad.write_bytes(b"\xff")
        with self.assertRaises(DiagnosticError):
            summarize_csv(bad, hashlib.sha256(bad.read_bytes()).hexdigest(), 123)
        self.assert_rejected(HEADER + "".join(rows([10, 11])).replace("vlc.exe", "vlc\x00.exe"))

    def test_cli_success_and_failure_remain_csv_only(self):
        path, digest = self.evidence()
        command = [sys.executable, str(ROOT / "scripts/b9_workload_diagnostic.py"),
                   str(path), digest, "123"]
        good = subprocess.run(command, capture_output=True, text=True)
        self.assertEqual(good.returncode, 0, good.stderr)
        self.assertFalse(json.loads(good.stdout)["criterion_pass"])
        bad = subprocess.run(command[:-2] + ["0" * 64, "123"], capture_output=True, text=True)
        self.assertEqual(bad.returncode, 1)
        result = json.loads(bad.stdout)
        self.assertEqual(result["diagnostic_class"], "INVALID_CSV")
        self.assertFalse(result["claim_eligible"])
        self.assertFalse(result["criterion_pass"])


if __name__ == "__main__":
    unittest.main(verbosity=2)
