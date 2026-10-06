#!/usr/bin/env python3
"""T20 seals and strict T1 reads coexist with generic non-T1 compatibility."""
from __future__ import annotations

import json
from pathlib import Path
import runpy
import tempfile
import unittest
from native_snapshot_restore_legacy_read_cases import LegacyReceiptReadCases, receipt

ROOT = Path(__file__).resolve().parents[2]
FIXTURE = runpy.run_path(str(ROOT / "tests/integration/native-snapshot-restore-receipt-seal-contract.py"))
TIER = "t20-a19-native-snapshot-restore"


def published(root: Path) -> Path:
    job, _ = FIXTURE["fixture"](root)
    result = FIXTURE["publish"](job)
    if result.returncode:
        raise AssertionError(result.stderr)
    return job


class NativeSnapshotRestoreCliReadContract(LegacyReceiptReadCases, unittest.TestCase):
    def test_valid_done_t20(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            job = published(root)
            result = receipt(root, job.name)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(json.loads(result.stdout),
                             json.loads((job / "receipt.public.json").read_bytes()))

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
if __name__ == "__main__":
    unittest.main()
