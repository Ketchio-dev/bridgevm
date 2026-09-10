#!/usr/bin/env python3
"""Headless contracts only: no guest, ETW, CGL, or live B6 proof."""
import hashlib
import json
import os
import pathlib
import subprocess
import sys
import tempfile
import threading
import time
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
import b6_presentmon_evidence as evidence

CSV = (
    "Application,ProcessID,SwapChainAddress,CPUStartTime,FrameTime,CPUBusy,CPUWait\n"
    "dwm.exe,44,0x1,0,10,9,1\n"
    "dwm.exe,44,0x1,10,12,10,2\n"
).encode()
HASH = hashlib.sha256(CSV).hexdigest()
NAME = "presentmon-classic-run1.csv"


class EvidenceContracts(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = pathlib.Path(self.tmp.name)
        self.csv = self.root / NAME
        self.log = self.root / "run.log"

    def report(self, digest=HASH, name=NAME):
        return ("BVPRESENTMON path=C:\\BridgeVMClosure\\" + name +
                " rows=3 data_rows=2 sha256=" + digest + " session=unique\r\n").encode()

    def test_fresh_hash_excludes_old_guest_report(self):
        old = self.report("0" * 64)
        self.log.write_bytes(old + self.report())
        self.assertEqual(evidence.guest_hash(self.log, len(old), NAME), HASH)

    def test_conflicting_hashes_are_rejected(self):
        self.log.write_bytes(self.report() + self.report("0" * 64))
        with self.assertRaises(ValueError):
            evidence.guest_hash(self.log, 0, NAME)

    def test_wrong_filename_or_prefix_cannot_authenticate(self):
        for raw in (self.report(name="prefix-" + NAME),
                    self.report().replace(b"BVPRESENTMON ", b"OTHER "),
                    self.report(name=NAME + ".old")):
            self.log.write_bytes(raw)
            with self.assertRaises(ValueError):
                evidence.guest_hash(self.log, 0, NAME)

    def test_negative_offset_is_rejected(self):
        self.log.write_bytes(self.report())
        with self.assertRaises(ValueError):
            evidence.guest_hash(self.log, -1, NAME)

    def test_repeated_identical_reports_are_consistent(self):
        self.log.write_bytes(self.report() * 2)
        self.assertEqual(evidence.guest_hash(self.log, 0, NAME), HASH)

    def test_waits_for_final_bytes_not_first_partial_transfer(self):
        self.csv.write_bytes(CSV[:20])
        thread = threading.Thread(target=lambda: (time.sleep(0.04), self.csv.write_bytes(CSV)))
        thread.start()
        try:
            report = evidence.wait_sample(self.csv, HASH, timeout=1, interval=0.01)
        finally:
            thread.join()
        self.assertFalse(report["claim_eligible"])
        self.assertFalse(report["criterion_pass"])
        self.assertFalse(report["capability_promotion"])

    def test_wrong_bytes_time_out(self):
        self.csv.write_bytes(b"stale")
        with self.assertRaises(TimeoutError):
            evidence.wait_sample(self.csv, HASH, timeout=0.01, interval=0.005)

    def test_symlink_is_refused_immediately(self):
        source = self.root / "source"
        source.write_bytes(CSV)
        self.csv.symlink_to(source)
        with self.assertRaises(ValueError):
            evidence.wait_sample(self.csv, HASH)

    def test_authenticated_invalid_csv_is_not_accepted(self):
        raw = b"FrameTime\ninvalid\n"
        self.csv.write_bytes(raw)
        with self.assertRaises(ValueError):
            evidence.wait_sample(self.csv, hashlib.sha256(raw).hexdigest())

    def driver(self, mode="ok", existing=False):
        out = self.root / "out"
        (out / "share").mkdir(parents=True)
        if existing:
            (out / "share" / NAME).write_bytes(b"old")
        self.log.write_bytes(b"")
        fixture = self.root / "fixture.csv"
        fixture.write_bytes(CSV)
        env = dict(os.environ, REPO=str(ROOT), OUT=str(out), RUN_LOG=str(self.log),
                   INPUT=str(self.root / "input.ctl"), FIXTURE=str(fixture),
                   MODE=mode, GUEST_HASH=HASH, PRESENTMON_NAME="PresentMon With Space.exe")
        shell = r"""
set -euo pipefail
: > "$INPUT"
LAUNCHER=$$
source "$REPO/scripts/b6-active-frame-time.sh"
foreground_hwnd() { if [[ "$MODE" == focus ]]; then printf 99; else printf 77; fi; }
wait_baseline() { printf 0; }
wait_after() { [[ "$MODE" != ack ]]; }
send_ok() {
  [[ "$MODE" != send ]] || return 1
  printf '%s\n' "$1" > "$OUT/command.txt"
  sleep 0.15
  cp "$FIXTURE" "$OUT/share/presentmon-classic-run1.csv"
  printf 'BVPRESENTMON path=C:\\BridgeVMClosure\\presentmon-classic-run1.csv sha256=%s\n' "$GUEST_HASH" >> "$RUN_LOG"
}
if b6_collect_active_frame_time classic 1 77; then exit 0; else exit 1; fi
"""
        result = subprocess.run(["bash", "-c", shell], env=env, capture_output=True, timeout=10)
        return result, out

    def test_driver_changes_scene_and_authenticates_guest_hash(self):
        result, out = self.driver()
        self.assertEqual(result.returncode, 0, result.stderr.decode())
        report = json.loads((out / "presentmon-classic-run1.frame-times.json").read_text())
        self.assertEqual(report["guest_reported_sha256"], HASH)
        self.assertGreater(report["host_input_commands"], 0)
        self.assertFalse(report["criterion_pass"])
        self.assertIn("KEY text-hex:58", (self.root / "input.ctl").read_text())
        self.assertIn('"C:\\BridgeVMClosure\\PresentMon With Space.exe"', (out / "command.txt").read_text())

    def test_failed_agent_command_is_not_success(self):
        result, out = self.driver(mode="send")
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse((out / "presentmon-classic-run1.frame-times.json").exists())

    def test_focus_refusal_sends_no_input(self):
        result, _ = self.driver(mode="focus")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual((self.root / "input.ctl").read_bytes(), b"")

    def test_existing_csv_is_not_overwritten(self):
        result, out = self.driver(existing=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual((out / "share" / NAME).read_bytes(), b"old")

    def test_missing_input_ack_is_not_success(self):
        result, out = self.driver(mode="ack")
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse((out / "presentmon-classic-run1.frame-times.json").exists())


if __name__ == "__main__":
    unittest.main()
