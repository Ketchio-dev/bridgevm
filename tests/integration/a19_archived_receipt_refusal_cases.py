"""Negative archive reads use real on-disk files and the actual CLI dispatcher."""
import json
from pathlib import Path
import tempfile

from a19_archived_receipt_fixtures import fixture, replace
import a19_archived_receipt_read as archive


class ArchivedRefusalCases:
    def test_missing_unsafe_or_malformed_public_and_private_receipts(self):
        for number in (21, 22):
            for filename in ("receipt.json", "receipt.public.json"):
                for mutation in ("missing", "symlink", "oversize", "json", "duplicate", "nonfinite", "schema"):
                    with self.subTest(tier=number, file=filename, mutation=mutation), tempfile.TemporaryDirectory() as temporary:
                        root = Path(temporary)
                        job, value = fixture(root, number)
                        path = job / filename
                        if mutation == "missing":
                            path.unlink()
                        elif mutation == "symlink":
                            saved = job / "saved"
                            path.rename(saved)
                            path.symlink_to(saved)
                        elif mutation == "oversize":
                            path.write_bytes(b" " * 65_537)
                        elif mutation == "json":
                            path.write_text("{")
                        elif mutation == "duplicate":
                            path.write_text('{"tier":"t8-pointer-reliability",' + json.dumps(value)[1:])
                        elif mutation == "nonfinite":
                            path.write_text(json.dumps({**value, "started_at": float("nan")}))
                        else:
                            path.write_text(json.dumps({**value, "sample_count": 99}))
                        self.refused(root, job)

    def test_seal_mutation_or_missing_ledger_refuses_even_matching_receipts(self):
        for number in (21, 22):
            for mutation in ("commit", "manifest", "binary", "missing", "writable", "ledger-symlink", "job-symlink"):
                with self.subTest(tier=number, mutation=mutation), tempfile.TemporaryDirectory() as temporary:
                    root = Path(temporary)
                    job, value = fixture(root, number)
                    entry = root / "queue/job-ledger" / job.name / "entry.env"
                    if mutation in ("commit", "manifest", "binary"):
                        key = {"commit": "commit", "manifest": "input_manifest_sha256", "binary": "binary_hash"}[mutation]
                        old = value[key]
                        for path in (job / "job.env", entry):
                            replace(path, old, "1" * len(old))
                    elif mutation == "missing":
                        entry.unlink()
                    elif mutation == "writable":
                        entry.chmod(0o600)
                    else:
                        path = entry if mutation == "ledger-symlink" else job / "job.env"
                        saved = path.with_name("saved")
                        path.rename(saved)
                        path.symlink_to(saved)
                    self.refused(root, job)

    def test_running_placement_is_not_an_authenticated_archive(self):
        for number in (21, 22):
            with self.subTest(tier=number), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                job, _ = fixture(root, number)
                target = root / "queue/running" / job.name
                target.parent.mkdir()
                job.rename(target)
                self.refused(root, target)

    def test_each_owned_residue_including_dangling_alias_is_refused(self):
        for number in (21, 22):
            tier = "t21-a19-quota-refusal" if number == 21 else "t22-a19-interrupted-restore"
            for name in archive.OWNED[tier]:
                for kind in ("directory", "dangling"):
                    with self.subTest(tier=number, name=name, kind=kind), tempfile.TemporaryDirectory() as temporary:
                        root = Path(temporary)
                        job, _ = fixture(root, number)
                        path = job / name
                        if kind == "directory":
                            path.mkdir()
                        else:
                            path.symlink_to(job / "absent-target")
                        self.refused(root, job)
