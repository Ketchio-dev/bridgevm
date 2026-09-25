#!/usr/bin/env python3
"""Retained B9 receipts must prove ordered guest focus and input events."""

from __future__ import annotations

import hashlib
import importlib.util
from pathlib import Path
import sys
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/live-gates"))
from b9_real_workload_receipt import validate_private

FIXTURE = ROOT / "tests/integration/b9-real-workload-pilot-contract.py"
spec = importlib.util.spec_from_file_location("b9_pilot_fixture", FIXTURE)
assert spec is not None and spec.loader is not None
pilot = importlib.util.module_from_spec(spec)
spec.loader.exec_module(pilot)


class RawFocusOrderContract(unittest.TestCase):
    def setUp(self):
        case = pilot.B9PilotContract("test_raw_pid_capture_receipt_and_public_no_claim")
        case.setUp()
        self.addCleanup(case.doCleanups)
        self.diagnostic, self.receipt = case.fixture()
        self.job = case.job
        self.log = self.diagnostic / "raw/guest/run.log"

    def rewrite(self, change):
        lines = self.log.read_text(encoding="utf-8").splitlines()
        change(lines)
        raw = ("\r\n".join(lines) + "\r\n").encode()
        self.log.write_bytes(raw)
        self.receipt["private_artifacts"]["run.log"] = {
            "bytes": len(raw), "sha256": hashlib.sha256(raw).hexdigest()}

    def test_valid_completed_focus_order(self):
        validate_private(self.receipt, self.job, self.diagnostic)

    def test_key_before_focus_is_rejected_after_rehash(self):
        def reorder(lines):
            key = lines.pop(next(i for i, line in enumerate(lines)
                                 if line.startswith("live input accepted: command=Key(")))
            lines.insert(1, key)
        self.rewrite(reorder)
        with self.assertRaisesRegex(ValueError, "order differs"):
            validate_private(self.receipt, self.job, self.diagnostic)

    def test_duplicate_foreground_is_rejected_after_rehash(self):
        def duplicate(lines):
            foreground = "B9-FOREGROUND-" + str(self.receipt["window_handle"])
            lines.insert(lines.index(foreground), foreground)
        self.rewrite(duplicate)
        with self.assertRaisesRegex(ValueError, "order differs"):
            validate_private(self.receipt, self.job, self.diagnostic)

    def test_wrong_hwnd_focus_attempt_is_rejected_after_rehash(self):
        def insert_wrong_focus(lines):
            lines.insert(1, "BVAGENT WINFOCUS 778 -> OK WINFOCUS")
        self.rewrite(insert_wrong_focus)
        with self.assertRaisesRegex(ValueError, "order differs"):
            validate_private(self.receipt, self.job, self.diagnostic)

    def test_missing_completed_command_end_is_rejected_after_rehash(self):
        self.rewrite(lambda lines: lines.remove(next(
            line for line in lines if line.startswith("BVAGENT END powershell.exe -NoProfile -EncodedCommand "))))
        with self.assertRaisesRegex(ValueError, "envelope differs"):
            validate_private(self.receipt, self.job, self.diagnostic)


if __name__ == "__main__":
    unittest.main(verbosity=2)
