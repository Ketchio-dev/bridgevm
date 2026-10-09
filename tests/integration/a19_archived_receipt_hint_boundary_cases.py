"""Unreadable public tier hints must not select the legacy raw-output path."""
import json
import os
from pathlib import Path
import tempfile

from a19_archived_receipt_fixtures import fixture, replace
from a19_archived_receipt_hint_cases import ArchivedHintCases
from native_snapshot_restore_legacy_read_cases import LEGACY_TIER, legacy_job, receipt


class ArchivedHintBoundaryCases(ArchivedHintCases):
    def test_public_only_hint_read_or_parse_failure_cannot_downgrade(self):
        for number in (21, 22):
            for mutation in ("oversize", "late-tier", "symlink", "truncated", "invalid-utf8", "deep"):
                with self.subTest(tier=number, mutation=mutation), tempfile.TemporaryDirectory() as temporary:
                    root = Path(temporary)
                    job, value = fixture(root, number)
                    for path in (job / "job.env", root / "queue/job-ledger" / job.name / "entry.env"):
                        replace(path, value["tier"], LEGACY_TIER)
                    public = job / "receipt.public.json"
                    changed = json.dumps({**value, "app_artifact_sha256": "1" * 64}).encode()
                    if mutation == "oversize":
                        public.write_bytes(changed.ljust(65_537, b" "))
                    elif mutation == "late-tier":
                        public.write_bytes(b'{"padding":"' + b"x" * 65_536 + b'","tier":'
                                           + json.dumps(value["tier"]).encode() + b"}")
                    elif mutation == "symlink":
                        target = job / "changed-public"
                        target.write_bytes(changed)
                        public.unlink()
                        public.symlink_to(target)
                    elif mutation == "truncated":
                        public.write_bytes(changed[:-1])
                    elif mutation == "invalid-utf8":
                        public.write_bytes(changed + b"\xff")
                    else:
                        public.write_bytes(b'{"nested":' + b"[" * 2000 + b"0" + b"]" * 2000
                                           + b',"tier":' + json.dumps(value["tier"]).encode() + b"}")
                    self.refused(root, job)

    def test_public_only_fifo_hint_refuses_without_waiting_for_writer(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            job, value = fixture(root, 21)
            for path in (job / "job.env", root / "queue/job-ledger" / job.name / "entry.env"):
                replace(path, value["tier"], LEGACY_TIER)
            public = job / "receipt.public.json"
            public.unlink()
            os.mkfifo(public)
            self.refused(root, job)

    def test_bounded_generic_receipt_preserves_exact_legacy_bytes(self):
        base = json.dumps({"tier": LEGACY_TIER, "pass": True}).encode()
        for raw in (base + b"\n", base.ljust(65_536, b" ")):
            with self.subTest(size=len(raw)), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                job, _ = legacy_job(root, LEGACY_TIER, "a" * 7, "done", "absent", raw)
                result = receipt(root, job.name)
                self.assertEqual((result.returncode, result.stdout), (0, raw), result.stderr)
