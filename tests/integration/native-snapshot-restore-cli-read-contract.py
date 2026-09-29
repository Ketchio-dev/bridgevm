#!/usr/bin/env python3
"""T20 CLI reads must revalidate published bytes; legacy reads stay unchanged."""
from __future__ import annotations

import json
import os
from pathlib import Path
import runpy
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
FIXTURE = runpy.run_path(str(ROOT / "tests/integration/native-snapshot-restore-receipt-seal-contract.py"))
TIER = "t20-a19-native-snapshot-restore"


def receipt(root: Path, job_id: str) -> subprocess.CompletedProcess[bytes]:
    return subprocess.run(
        ["bash", str(ROOT / "scripts/live-gates/bridgevm-live"), "receipt", job_id],
        capture_output=True, env=dict(os.environ, BRIDGEVM_LIVE_ROOT=str(root / "queue")),
        timeout=10,
    )


def published(root: Path) -> Path:
    job, _ = FIXTURE["fixture"](root)
    result = FIXTURE["publish"](job)
    if result.returncode:
        raise AssertionError(result.stderr)
    return job


class NativeSnapshotRestoreCliReadContract(unittest.TestCase):
    def test_valid_done_t20_and_legacy_running_without_ledger(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            job = published(root)
            result = receipt(root, job.name)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(json.loads(result.stdout),
                             json.loads((job / "receipt.public.json").read_bytes()))
            legacy = root / "queue/running/legacy-t1"
            legacy.mkdir(parents=True)
            (legacy / "job.env").write_text(
                "job_id=legacy-t1\ntier=t1-vtimer\ncommit=" + "a" * 40 + "\n")
            raw = b'{"tier":"t1-vtimer","pass":true}\n'
            (legacy / "receipt.public.json").write_bytes(raw)
            result = receipt(root, legacy.name)
            self.assertEqual((result.returncode, result.stdout), (0, raw), result.stderr)
            (root / "queue/job-ledger" / legacy.name).mkdir(parents=True)
            result = receipt(root, legacy.name)
            self.assertEqual((result.returncode, result.stdout), (0, raw), result.stderr)
            done = root / "queue/done" / legacy.name
            legacy.rename(done)
            result = receipt(root, done.name)
            self.assertEqual((result.returncode, result.stdout), (0, raw), result.stderr)
            # Historical receipts may predate full-SHA queue identities.
            (done / "job.env").write_text(
                "job_id=legacy-t1\ntier=t1-vtimer\ncommit=" + "a" * 7 + "\n")
            result = receipt(root, done.name)
            self.assertEqual((result.returncode, result.stdout), (0, raw), result.stderr)

    def test_changed_public_or_private_is_never_served(self):
        for mutation in ("public", "public-tier", "private", "missing-private",
                         "missing-ledger", "malformed-ledger", "malformed-job"):
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                job = published(root)
                if mutation == "public":
                    (job / "receipt.public.json").write_text(
                        '{"tier":"' + TIER + '","private_path":"/secret"}\n')
                elif mutation == "public-tier":
                    public = job / "receipt.public.json"
                    public.write_text(public.read_text().replace(TIER, "t1-vtimer"))
                elif mutation == "private":
                    private = json.loads((job / "receipt.json").read_text())
                    private["pass"] = False
                    (job / "receipt.json").write_text(json.dumps(private))
                elif mutation == "missing-private":
                    (job / "receipt.json").unlink()
                elif mutation == "missing-ledger":
                    (root / "queue/job-ledger" / job.name / "entry.env").unlink()
                elif mutation == "malformed-ledger":
                    entry = root / "queue/job-ledger" / job.name / "entry.env"
                    entry.chmod(0o600)
                    entry.write_text("tier=" + TIER + "\ntier=" + TIER + "\n")
                    entry.chmod(0o400)
                else:
                    (job / "job.env").write_text("tier=" + TIER + "\ntier=" + TIER + "\n")
                result = receipt(root, job.name)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(result.stdout, b"")
                self.assertNotIn(b"/secret", result.stdout)

    def test_t20_never_serves_before_done_or_through_noncanonical_id(self):
        for state in ("queued", "running"):
            with self.subTest(state=state), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                job = published(root)
                destination = root / "queue" / state / job.name
                destination.parent.mkdir()
                job.rename(destination)
                result = receipt(root, job.name)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(result.stdout, b"")
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            job = published(root)
            (root / "queue/queued").mkdir()
            result = receipt(root, "../done/" + job.name)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(result.stdout, b"")

    def test_one_changed_identity_cannot_downgrade_t20_to_legacy(self):
        for mutation in ("job", "ledger", "public"):
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                job = published(root)
                entry = root / "queue/job-ledger" / job.name / "entry.env"
                if mutation == "job":
                    path = job / "job.env"
                    path.write_text(path.read_text().replace(TIER, "t1-vtimer"))
                elif mutation == "ledger":
                    entry.chmod(0o600)
                    entry.write_text(entry.read_text().replace(TIER, "t1-vtimer"))
                    entry.chmod(0o400)
                else:
                    for path in (job / "job.env", entry):
                        path.chmod(0o600)
                        path.write_text(path.read_text().replace(TIER, "t1-vtimer"))
                    entry.chmod(0o400)
                    (job / "receipt.public.json").write_text(
                        '{"tier":"' + TIER + '","private_path":"/secret"}\n')
                result = receipt(root, job.name)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(result.stdout, b"")

    def test_legacy_with_present_mismatched_ledger_is_refused(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            job = root / "queue/running/legacy-t1"
            job.mkdir(parents=True)
            (job / "job.env").write_text(
                "job_id=legacy-t1\ntier=t1-vtimer\ncommit=" + "a" * 40 + "\n")
            (job / "receipt.public.json").write_bytes(b'{"tier":"t1-vtimer"}\n')
            ledger = root / "queue/job-ledger/legacy-t1"
            ledger.mkdir(parents=True)
            (ledger / "entry.env").write_text(
                "job_id=legacy-t1\ntier=t1-vtimer\ncommit=" + "b" * 40 + "\n")
            (ledger / "entry.env").chmod(0o400)
            result = receipt(root, job.name)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(result.stdout, b"")


if __name__ == "__main__":
    unittest.main()
