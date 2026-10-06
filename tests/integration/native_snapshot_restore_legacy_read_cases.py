"""Generic compatibility and strict T1 refusals through the actual CLI reader."""
import json
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]
LEGACY_TIER = "t8-pointer-reliability"


def receipt(root: Path, job_id: str) -> subprocess.CompletedProcess[bytes]:
    return subprocess.run(
        ["bash", str(ROOT / "scripts/live-gates/bridgevm-live"), "receipt", job_id],
        capture_output=True, env=dict(os.environ, BRIDGEVM_LIVE_ROOT=str(root / "queue")),
        timeout=10,
    )


def legacy_job(root, tier, commit, state, ledger, raw):
    job = root / "queue" / state / "legacy-fixture"
    job.mkdir(parents=True)
    identity = f"job_id={job.name}\ntier={tier}\ncommit={commit}\n"
    (job / "job.env").write_text(identity)
    (job / "receipt.public.json").write_bytes(raw)
    entry = root / "queue/job-ledger" / job.name / "entry.env"
    if ledger != "absent":
        entry.parent.mkdir(parents=True)
        if ledger == "matching":
            entry.write_text(identity)
            entry.chmod(0o400)
    return job, entry


class LegacyReceiptReadCases:
    def test_non_t1_generic_legacy_reads_with_optional_ledger(self):
        raw = json.dumps({"tier": LEGACY_TIER, "pass": True}).encode() + b"\n"
        for state in ("running", "done"):
            for commit in ("a" * 40, "a" * 7):
                for ledger in ("absent", "empty", "matching"):
                    with self.subTest(state=state, commit=commit, ledger=ledger), tempfile.TemporaryDirectory() as temporary:
                        root = Path(temporary)
                        job, _ = legacy_job(root, LEGACY_TIER, commit, state, ledger, raw)
                        result = receipt(root, job.name)
                        self.assertEqual((result.returncode, result.stdout), (0, raw), result.stderr)

    def test_non_t1_mismatched_ledger_alone_refuses_previously_valid_read(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            raw = json.dumps({"tier": LEGACY_TIER, "pass": True}).encode() + b"\n"
            job, entry = legacy_job(root, LEGACY_TIER, "a" * 40, "running", "matching", raw)
            result = receipt(root, job.name)
            self.assertEqual((result.returncode, result.stdout), (0, raw), result.stderr)
            entry.chmod(0o600)
            entry.write_text(entry.read_text().replace("a" * 40, "b" * 40))
            entry.chmod(0o400)
            result = receipt(root, job.name)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(result.stdout, b"")
            self.assertIn(b"differs from its ledger", result.stderr)

    def test_bare_t1_is_refused_with_optional_ledger_and_historical_commit(self):
        raw = b'{"tier":"t1-vtimer","pass":true}\n'
        for state in ("running", "done"):
            for commit in ("a" * 40, "a" * 7):
                for ledger in ("absent", "empty", "matching"):
                    with self.subTest(state=state, commit=commit, ledger=ledger), tempfile.TemporaryDirectory() as temporary:
                        root = Path(temporary)
                        job, _ = legacy_job(root, "t1-vtimer", commit, state, ledger, raw)
                        result = receipt(root, job.name)
                        self.assertNotEqual(result.returncode, 0)
                        self.assertEqual(result.stdout, b"")
                        self.assertIn(b"T1 receipt schema differs", result.stderr)

    def test_a3_receipt_cannot_be_copied_under_t1_queue_identity(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            raw = json.dumps({"tier": "t6-a3-title", "criterion": "A3", "pass": True,
                              "tested_commit": "a" * 40}).encode() + b"\n"
            job, entry = legacy_job(root, "t6-a3-title", "a" * 40, "running", "matching", raw)
            result = receipt(root, job.name)
            self.assertEqual((result.returncode, result.stdout), (0, raw), result.stderr)
            for path in (job / "job.env", entry):
                path.chmod(0o600)
                path.write_text(path.read_text().replace("t6-a3-title", "t1-vtimer"))
            entry.chmod(0o400)
            result = receipt(root, job.name)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(result.stdout, b"")
            self.assertIn(b"T1 queue tier differs", result.stderr)
