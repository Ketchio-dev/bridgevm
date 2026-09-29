#!/usr/bin/env python3
"""Reject T20 receipts detached from the job selected for direct verification."""
from __future__ import annotations

import json
from pathlib import Path
import runpy
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
helpers = runpy.run_path(str(ROOT / "tests/integration/native-snapshot-restore-receipt-seal-contract.py"))
COMMIT = helpers["COMMIT"]


class NativeSnapshotRestoreCliPathContract(unittest.TestCase):
    def test_forged_success_cannot_borrow_failed_jobs_seal(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            job, forged = helpers["fixture"](root)
            failed = helpers["initial"](job.name, COMMIT)
            failed.update({"started_at": "2026-09-28T00:00:00+00:00",
                           "finished_at": "2026-09-28T00:00:01+00:00",
                           "input_manifest_sha256": helpers["MANIFEST"],
                           "binary_hash": helpers["BINARY"],
                           "worker_cleanup_verified": True})
            actual = job / "receipt.json"
            actual.write_text(json.dumps(failed) + "\n", encoding="utf-8")
            elsewhere = root / "elsewhere"
            elsewhere.mkdir()
            moved = elsewhere / "receipt.public.json"
            moved.write_text(json.dumps(forged) + "\n", encoding="utf-8")
            alternate = job / "forged.json"
            alternate.write_text(json.dumps(forged) + "\n", encoding="utf-8")
            public = job / "receipt.public.json"
            public.write_text(json.dumps(forged) + "\n", encoding="utf-8")

            def verify(path: Path) -> subprocess.CompletedProcess[str]:
                return subprocess.run(
                    [sys.executable, str(ROOT / "scripts/live-gates/native_snapshot_restore_receipt.py"),
                     "verify", str(path), "--expected-commit", COMMIT, "--job-dir", str(job)],
                    capture_output=True, text=True, timeout=10)

            self.assertEqual(verify(actual).returncode, 0)
            for candidate in (moved, alternate, public):
                with self.subTest(candidate=candidate):
                    self.assertNotEqual(verify(candidate).returncode, 0)


if __name__ == "__main__":
    unittest.main()
