"""Malformed strict hints cannot disappear into the generic byte-stream reader."""
import json
from pathlib import Path
import tempfile

from a19_archived_receipt_fixtures import fixture, replace

class ArchivedHintCases:
    def test_any_remaining_tier_hint_prevents_legacy_downgrade(self):
        for number in (21, 22):
            for remaining in ("job", "ledger", "public"):
                with self.subTest(tier=number, remaining=remaining), tempfile.TemporaryDirectory() as temporary:
                    root = Path(temporary)
                    job, value = fixture(root, number)
                    paths = {"job": job / "job.env", "public": job / "receipt.public.json",
                             "ledger": root / "queue/job-ledger" / job.name / "entry.env"}
                    for hint, path in paths.items():
                        if hint != remaining:
                            replace(path, value["tier"], "t8-pointer-reliability")
                    self.refused(root, job)

    def test_duplicate_or_nonfinite_public_only_hint_cannot_downgrade(self):
        for number in (21, 22):
            for mutation in ("duplicate-first", "duplicate-last", "nonfinite", "nested"):
                with self.subTest(tier=number, mutation=mutation), tempfile.TemporaryDirectory() as temporary:
                    root = Path(temporary)
                    job, value = fixture(root, number)
                    for path in (job / "job.env", root / "queue/job-ledger" / job.name / "entry.env"):
                        replace(path, value["tier"], "t8-pointer-reliability")
                    strict = json.dumps({"tier": value["tier"], "pass": True})
                    if mutation == "duplicate-first":
                        raw = '{"tier":"t8-pointer-reliability",' + strict[1:]
                    elif mutation == "duplicate-last":
                        raw = strict[:-1] + ',"tier":"t8-pointer-reliability"}'
                    else:
                        raw = (json.dumps({"tier": "t8-pointer-reliability", "nested": {"tier": value["tier"]}})
                               if mutation == "nested" else strict[:-1] + ',"extra":NaN}')
                    (job / "receipt.public.json").write_text(raw)
                    self.refused(root, job)

    def test_matching_invalid_receipts_cannot_claim_promotion(self):
        for number in (21, 22):
            for field in ("claim_eligible", "criterion_pass", "capability_promotion", "three_d_injection"):
                with self.subTest(tier=number, field=field), tempfile.TemporaryDirectory() as temporary:
                    root = Path(temporary)
                    job, value = fixture(root, number)
                    raw = json.dumps({**value, field: True})
                    for name in ("receipt.json", "receipt.public.json"):
                        (job / name).write_text(raw)
                    self.refused(root, job)
