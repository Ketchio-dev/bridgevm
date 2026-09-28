#!/usr/bin/env python3
"""Synthetic T20 queue-seal verification and publication contracts."""
from __future__ import annotations

import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/live-gates"))
from native_snapshot_restore_receipt import HASHES, TIER, initial
from native_snapshot_restore_seal import merge_prepared, sealed_hashes

COMMIT = "a" * 40
MANIFEST = "b" * 64
BINARY = "c" * 64


def passing(job_id: str, commit: str = COMMIT) -> dict:
    receipt = initial(job_id, commit)
    receipt.update({key: "d" * 64 for key in HASHES})
    receipt.update({
        "input_manifest_sha256": MANIFEST, "binary_hash": BINARY,
        "started_at": "2026-09-28T00:00:00+00:00",
        "finished_at": "2026-09-28T00:00:01+00:00",
        "host_model": "Mac17,9", "macos_version": "26.0",
        "outcome": "completed", "pass": True, "boots_attempted": 3,
        "boots_passed": 3, "natural_shutdown_count": 3, "run_count": 1,
        "sample_count": 1, "clobber_marker_sha256": "e" * 64,
        "worker_cleanup_verified": True,
    })
    return receipt


def fixture(root: Path, job_id: str = "synthetic-t20", commit: str = COMMIT) -> tuple[Path, dict]:
    job = root / "queue/done" / job_id
    ledger = root / "queue/job-ledger" / job_id
    job.mkdir(parents=True)
    ledger.mkdir(parents=True)
    rows = {"job_id": job_id, "tier": TIER, "commit": commit,
            "input_manifest_sha256": MANIFEST, "sealed_binary_sha256": BINARY}
    content = "".join(f"{key}={value}\n" for key, value in rows.items())
    (job / "job.env").write_text(content + "submitted_at=synthetic\n", encoding="utf-8")
    entry = ledger / "entry.env"
    entry.write_text(content, encoding="utf-8")
    entry.chmod(0o400)
    receipt = passing(job_id, commit)
    (job / "receipt.json").write_text(json.dumps(receipt) + "\n", encoding="utf-8")
    return job, receipt


def verify(path: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["bash", str(ROOT / "scripts/live-gates/verify-live-receipt.sh"),
         TIER, str(path), str(ROOT), COMMIT], capture_output=True, text=True, timeout=10,
    )


def publish(job: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["bash", str(ROOT / "scripts/live-gates/publish-receipt.sh"),
         TIER, str(job), str(ROOT), COMMIT], capture_output=True, text=True, timeout=10,
    )


class NativeSnapshotRestoreReceiptSealContract(unittest.TestCase):
    def test_sealed_receipt_verifies_and_publishes(self):
        with tempfile.TemporaryDirectory() as temporary:
            job, receipt = fixture(Path(temporary))
            self.assertEqual(sealed_hashes(job, receipt["job_id"], COMMIT),
                             {"input_manifest_sha256": MANIFEST, "binary_hash": BINARY})
            self.assertEqual(verify(job / "receipt.json").returncode, 0)
            result = publish(job)
            self.assertEqual(result.returncode, 0, result.stderr)
            public = job / "receipt.public.json"
            self.assertEqual(verify(public).returncode, 0)
            self.assertEqual(json.loads(public.read_text(encoding="utf-8")), receipt)

    def test_orphan_and_moved_receipts_cannot_verify_or_publish(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            job, _ = fixture(root)
            orphan = root / "orphan/done/synthetic-t20"
            orphan.mkdir(parents=True)
            shutil.copyfile(job / "receipt.json", orphan / "receipt.json")
            moved = root / "queue/done/different-job"
            moved.mkdir()
            shutil.copyfile(job / "receipt.json", moved / "receipt.json")
            for candidate in (orphan, moved):
                with self.subTest(candidate=candidate):
                    self.assertNotEqual(verify(candidate / "receipt.json").returncode, 0)
                    self.assertNotEqual(publish(candidate).returncode, 0)
                    self.assertFalse((candidate / "receipt.public.json").exists())

    def test_wrong_or_unsafe_queue_seal_is_rejected(self):
        for mutation in ("receipt-hash", "job-hash", "ledger-hash", "duplicate-job",
                         "missing-ledger", "writable-ledger", "symlink-ledger"):
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                job, receipt = fixture(root)
                entry = root / "queue/job-ledger/synthetic-t20/entry.env"
                if mutation == "receipt-hash":
                    receipt["binary_hash"] = "e" * 64
                    (job / "receipt.json").write_text(json.dumps(receipt), encoding="utf-8")
                elif mutation == "job-hash":
                    path = job / "job.env"
                    path.write_text(path.read_text().replace(MANIFEST, "e" * 64), encoding="utf-8")
                elif mutation == "ledger-hash":
                    entry.chmod(0o600)
                    entry.write_text(entry.read_text().replace(BINARY, "e" * 64), encoding="utf-8")
                    entry.chmod(0o400)
                elif mutation == "duplicate-job":
                    with (job / "job.env").open("a", encoding="utf-8") as output:
                        output.write("job_id=synthetic-t20\n")
                elif mutation == "missing-ledger":
                    entry.unlink()
                elif mutation == "writable-ledger":
                    entry.chmod(0o600)
                else:
                    entry.unlink()
                    entry.symlink_to(job / "job.env")
                self.assertNotEqual(verify(job / "receipt.json").returncode, 0)
                self.assertNotEqual(publish(job).returncode, 0)
                self.assertFalse((job / "receipt.public.json").exists())

    def test_prepared_inputs_must_match_seal_and_direct_verify_needs_job_dir(self):
        with tempfile.TemporaryDirectory() as temporary:
            job, receipt = fixture(Path(temporary))
            prepared = {"input_manifest_sha256": MANIFEST, "binary_hash": BINARY}
            merge_prepared(receipt, prepared)
            with self.assertRaises(ValueError):
                merge_prepared(receipt, {**prepared, "binary_hash": "e" * 64})
            direct = subprocess.run(
                [sys.executable, str(ROOT / "scripts/live-gates/native_snapshot_restore_receipt.py"),
                 "verify", str(job / "receipt.json"), "--expected-commit", COMMIT],
                capture_output=True, text=True, timeout=10,
            )
            self.assertNotEqual(direct.returncode, 0)

    def test_symlink_and_oversized_receipt_are_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            job, _ = fixture(Path(temporary))
            private = job / "receipt.json"
            linked = job / "linked.json"
            linked.symlink_to(private)
            self.assertNotEqual(verify(linked).returncode, 0)
            private.write_bytes(private.read_bytes() + b" " * 65_536)
            self.assertNotEqual(verify(private).returncode, 0)

    def test_failed_clean_receipt_publishes_without_a_claim(self):
        with tempfile.TemporaryDirectory() as temporary:
            job, _ = fixture(Path(temporary))
            failed = initial(job.name, COMMIT)
            failed.update({"started_at": "2026-09-28T00:00:00+00:00",
                           "finished_at": "2026-09-28T00:00:01+00:00",
                           "input_manifest_sha256": MANIFEST, "binary_hash": BINARY,
                           "worker_cleanup_verified": True})
            (job / "receipt.json").write_text(json.dumps(failed), encoding="utf-8")
            self.assertEqual(verify(job / "receipt.json").returncode, 0)
            self.assertEqual(publish(job).returncode, 0)
            public = json.loads((job / "receipt.public.json").read_text(encoding="utf-8"))
            self.assertFalse(public["pass"])
            self.assertEqual((public["sample_count"], public["run_count"]), (0, 0))

    def test_owned_media_residue_blocks_verification_and_publication(self):
        for name in ("prepared-inputs", "live"):
            with self.subTest(name=name), tempfile.TemporaryDirectory() as temporary:
                job, _ = fixture(Path(temporary))
                (job / name).mkdir()
                self.assertNotEqual(verify(job / "receipt.json").returncode, 0)
                self.assertNotEqual(publish(job).returncode, 0)
                self.assertFalse((job / "receipt.public.json").exists())

    def test_repeated_json_fields_and_invalid_public_identity_are_rejected(self):
        mutations = (
            ("started_at", "not-utc"), ("finished_at", "2026-09-27T23:59:59Z"),
            ("started_at", "2026-02-30T00:00:00Z"), ("started_at", "2026-09-28T00:00:00-04:00"),
            ("host_model", "private/path"), ("macos_version", "26.0/private"),
            ("host_model", "Mac" + "x" * 80 + "1,1"),
        )
        for field, replacement in mutations:
            with self.subTest(field=field, replacement=replacement), tempfile.TemporaryDirectory() as temporary:
                job, receipt = fixture(Path(temporary))
                receipt[field] = replacement
                (job / "receipt.json").write_text(json.dumps(receipt), encoding="utf-8")
                self.assertNotEqual(verify(job / "receipt.json").returncode, 0)
                self.assertNotEqual(publish(job).returncode, 0)
        with tempfile.TemporaryDirectory() as temporary:
            job, receipt = fixture(Path(temporary))
            raw = json.dumps(receipt).replace('"tier":', '"tier": "forged", "tier":', 1)
            (job / "receipt.json").write_text(raw, encoding="utf-8")
            self.assertNotEqual(verify(job / "receipt.json").returncode, 0)
            self.assertNotEqual(publish(job).returncode, 0)
        with tempfile.TemporaryDirectory() as temporary:
            job, receipt = fixture(Path(temporary))
            raw = json.dumps(receipt).replace('"boots_attempted": 3', '"boots_attempted": NaN', 1)
            (job / "receipt.json").write_text(raw, encoding="utf-8")
            self.assertNotEqual(verify(job / "receipt.json").returncode, 0)
            self.assertNotEqual(publish(job).returncode, 0)


if __name__ == "__main__":
    unittest.main()
