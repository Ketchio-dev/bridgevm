#!/usr/bin/env python3
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
from b6_frame_times import MAX_BYTES, compare, summarize

HEADER = "Application,ProcessID,SwapChainAddress,CPUStartTime,FrameTime,CPUBusy,CPUWait\r\n"


class FrameTimeTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)

    def evidence(self, text=None, *, name="frames.csv", frame=10, pid=10):
        if text is None:
            text = HEADER + "".join(f"dwm.exe,{pid},0x20,{i * frame},{frame},{frame},0\r\n" for i in range(3))
        raw = ("\ufeff" + text).encode("utf-8")
        path = self.root / name
        path.write_bytes(raw)
        return path, hashlib.sha256(raw).hexdigest()

    def test_bom_crlf_summary_never_promotes(self):
        report, _ = summarize(*self.evidence())
        self.assertEqual((report["frame_count"], report["mean_ms"], report["cpu_start_span_ms"]), (3, 10, 20))
        self.assertFalse(report["criterion_pass"])
        self.assertFalse(report["claim_eligible"])

    def test_bad_hash_is_rejected(self):
        path, _ = self.evidence()
        with self.assertRaises(ValueError): summarize(path, "0" * 64)

    def test_missing_and_duplicate_columns(self):
        for header in (HEADER.replace("FrameTime", "GPUTime"), HEADER.replace("CPUWait", "FrameTime")):
            with self.assertRaises(ValueError): summarize(*self.evidence(header))

    def test_nonpositive_or_nonfinite_values(self):
        for value in ("0", "-1", "NaN", "inf", "-inf", "NA"):
            with self.assertRaises(ValueError):
                summarize(*self.evidence(HEADER + f"dwm.exe,10,0x20,0,{value},10,0\r\n"))

    def test_mixed_or_wrong_streams(self):
        first = "dwm.exe,10,0x20,0,10,10,0\r\n"
        for second in ("notepad.exe,10,0x20,10,10,10,0", "dwm.exe,11,0x20,10,10,10,0",
                       "dwm.exe,10,0x21,10,10,10,0", "dwm.exe,0,0x20,10,10,10,0"):
            with self.assertRaises(ValueError): summarize(*self.evidence(HEADER + first + second + "\r\n"))

    def test_ragged_or_extra_row_fields(self):
        for row in ("dwm.exe,10,0x20,0,10,10", "dwm.exe,10,0x20,0,10,10,0,extra"):
            with self.assertRaises(ValueError): summarize(*self.evidence(HEADER + row + "\r\n"))

    def test_timeline_gaps_and_reordering(self):
        for start in (0, 9, 20):
            text = HEADER + f"dwm.exe,10,0x20,0,10,10,0\r\ndwm.exe,10,0x20,{start},10,10,0\r\n"
            with self.assertRaises(ValueError): summarize(*self.evidence(text))

    def test_busy_wait_mismatch(self):
        with self.assertRaises(ValueError):
            summarize(*self.evidence(HEADER + "dwm.exe,10,0x20,0,10,1,1\r\n"))

    def test_empty_and_single_frame_are_not_comparisons(self):
        for text in (HEADER, HEADER + "dwm.exe,10,0x20,0,10,10,0\r\n"):
            with self.assertRaises(ValueError): summarize(*self.evidence(text))

    def test_symlink_and_oversized_file(self):
        path, digest = self.evidence()
        link = self.root / "link.csv"
        link.symlink_to(path)
        with self.assertRaises(ValueError): summarize(link, digest)
        with path.open("wb") as stream: stream.truncate(MAX_BYTES + 1)
        with self.assertRaises(ValueError): summarize(path, digest)

    def test_same_evidence_is_not_a_baseline_comparison(self):
        path, digest = self.evidence()
        with self.assertRaises(ValueError): compare(path, digest, path, digest)
        duplicate, duplicate_hash = self.evidence(name="duplicate.csv")
        with self.assertRaises(ValueError): compare(path, digest, duplicate, duplicate_hash)

    def test_exact_limit_and_regression_without_promotion(self):
        baseline, first = self.evidence()
        for frame, expected in ((11, True), (11.01, False), (9, True)):
            candidate, second = self.evidence(name="candidate.csv", frame=frame, pid=20)
            report = compare(baseline, first, candidate, second)
            self.assertEqual(report["mean_within_ten_percent"], expected)
            self.assertEqual(report["p95_within_ten_percent"], expected)
            self.assertFalse(report["criterion_pass"])
            self.assertFalse(report["claim_eligible"])

    def test_cli_reports_bad_hash_as_failure(self):
        path, _ = self.evidence()
        result = subprocess.run([sys.executable, str(ROOT / "scripts/analyze-b6-frame-times.py"),
                                 "summary", str(path), "0" * 64], capture_output=True, text=True)
        self.assertEqual(result.returncode, 1)
        self.assertFalse(json.loads(result.stdout)["valid"])


if __name__ == "__main__":
    unittest.main(verbosity=2)
