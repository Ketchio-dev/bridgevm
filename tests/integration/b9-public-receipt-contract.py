#!/usr/bin/env python3
"""Public B9 verification requires matching private and retained raw evidence."""

from __future__ import annotations

import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/live-gates"))
from b9_real_workload_queue import missing
import b9_real_workload_inputs as inputs

SPEC = importlib.util.spec_from_file_location(
    "b9_pilot_fixture", ROOT / "tests/integration/b9-real-workload-pilot-contract.py")
assert SPEC is not None and SPEC.loader is not None
pilot = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(pilot)


class PublicReceiptContract(unittest.TestCase):
    def setUp(self):
        case = pilot.B9PilotContract("test_raw_pid_capture_receipt_and_public_no_claim")
        case.setUp()
        self.addCleanup(case.doCleanups)
        self.case = case
        self.diagnostic, self.private = case.fixture()
        self.public = pilot.public_view(self.private, case.job)
        self.private_path = case.job_dir / "receipt.json"
        self.public_path = case.job_dir / "receipt.public.json"

    def verify(self, private=None, public=None):
        if private is not None:
            self.private_path.write_text(json.dumps(private), encoding="utf-8")
        if public is not None:
            self.public_path.write_text(json.dumps(public), encoding="utf-8")
        return subprocess.run(
            [sys.executable, str(ROOT / "scripts/live-gates/b9_real_workload_receipt.py"),
             "verify-public", str(self.case.job_dir), self.case.commit],
            capture_output=True, text=True, check=False)

    def test_matching_positive_and_cleaned_incomplete_verify_without_claim(self):
        result = self.verify(self.private, self.public)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse(any(self.public[flag] for flag in pilot.FLAGS))
        incomplete = missing(self.case.job)
        incomplete.update(cleanup_complete=True, owned_process_group_stopped=True)
        self.assertEqual(self.verify(incomplete, pilot.public_view(incomplete, self.case.job)).returncode, 0)

    def test_conflicting_class_or_altered_public_result_is_refused(self):
        self.verify(self.private, self.public)
        for field, changed in (("result_class", "PLAYBACK_INCOMPLETE"),
                               ("frame_count", self.public["frame_count"] + 1)):
            with self.subTest(field=field):
                public = copy.deepcopy(self.public)
                public[field] = changed
                self.assertNotEqual(self.verify(self.private, public).returncode, 0)

    def test_exported_numeric_and_boolean_sentinels_are_refused(self):
        for field, changed in (("target_pid", "not-a-pid"),
                               ("cpu_start_span_ms", "7000"),
                               ("p95_nearest_rank_ms", "1000"),
                               ("distinct_scanout_count", "3"),
                               ("guest_shutdown_observed", 1)):
            with self.subTest(field=field):
                public = copy.deepcopy(self.public)
                public[field] = changed
                result = self.verify(self.private, public)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("exported", result.stderr)

    def test_private_absence_or_changed_raw_bytes_is_refused(self):
        self.verify(self.private, self.public)
        self.private_path.unlink()
        self.assertNotEqual(self.verify().returncode, 0)
        self.verify(self.private, self.public)
        raw = self.diagnostic / "raw/guest/run.log"
        raw.write_bytes(raw.read_bytes() + b"forged guest tail\r\n")
        self.assertNotEqual(self.verify().returncode, 0)

    def test_staged_control_or_missing_part_is_refused(self):
        for changed in ("control", "part"):
            with self.subTest(changed=changed):
                private = copy.deepcopy(self.private)
                staged = private["staged_file_hashes"]
                if changed == "control":
                    staged["bv-b9-control.ps1"] = "0" * 64
                else:
                    del staged["b9-vlc-part-09.bin"]
                self.assertNotEqual(self.verify(private, self.public).returncode, 0)

    def test_shared_control_is_crlf_file_and_host_refuses_tamper(self):
        raw = (ROOT / "scripts/win-assets/bv-b9-control.ps1").read_bytes()
        self.assertEqual(raw.count(b"\n"), raw.count(b"\r\n"))
        self.assertLess(len(raw), 8_000_000)
        self.assertIn(b"Invoke-CimMethod -ClassName Win32_Process", raw)
        runner = (ROOT / "scripts/live-gates/run-b9-real-workload-pilot.py").read_text()
        self.assertNotIn("-EncodedCommand", runner)
        self.assertNotIn(' -Command "', runner)
        share = self.case.root / "share"
        share.mkdir()
        control = share / "bv-b9-control.ps1"
        control.write_bytes(raw)
        command = pilot.RUNNER.control_command(share, hashlib.sha256(raw).hexdigest(), "Firstboot")
        self.assertIn(" -File C:\\BridgeVMB9\\bv-b9-control.ps1 -Action Firstboot ", command)
        control.write_bytes(raw + b"tamper\r\n")
        with self.assertRaisesRegex(ValueError, "shared control bytes differ"):
            pilot.RUNNER.control_command(share, hashlib.sha256(raw).hexdigest(), "Firstboot")

    def test_committed_script_blob_differs_from_dirty_checkout_bytes(self):
        with tempfile.TemporaryDirectory() as directory:
            repo = Path(directory).resolve()
            subprocess.run(["git", "init", "-q", str(repo)], check=True)
            path = repo / "scripts/win-assets/bv-b9-control.ps1"
            path.parent.mkdir(parents=True)
            path.write_bytes(b"good\r\n")
            subprocess.run(["git", "-C", str(repo), "add", "--", str(path)], check=True)
            subprocess.run(["git", "-C", str(repo), "-c", "user.name=B9 Contract",
                            "-c", "user.email=b9@example.invalid", "commit", "-qm", "sealed"], check=True)
            path.write_bytes(b"changed\r\n")
            with patch.object(inputs, "REPO", repo):
                self.assertEqual(inputs.committed_blob_sha256(path), hashlib.sha256(b"good\r\n").hexdigest())
                self.assertNotEqual(inputs.committed_blob_sha256(path), inputs.stable_file(path)[1])

    def test_exact_head_pins_both_guest_scripts(self):
        for name in ("bv-b9-vlc-playback.ps1", "bv-b9-control.ps1"):
            path = ROOT / "scripts/win-assets" / name
            self.assertEqual(inputs.committed_blob_sha256(path), inputs.stable_file(path)[1])


if __name__ == "__main__":
    unittest.main(verbosity=2)
