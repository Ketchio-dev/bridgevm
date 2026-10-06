#!/usr/bin/env python3
"""Sealed development envelope refuses altered source and lineage without guests."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import unittest
from unittest.mock import patch

from t22_pair_fixtures import COMMIT
from t22_pair_worker_fixtures import WorkerFixture
from t22_pair_queue_inputs import parse, validate, bound, job, query_hash, SCRIPT, seal
from t22_pair_queue import require_source
from t22_pair_queue_receipt import empty, checked


class QueueInputs(unittest.TestCase):
    def setUp(self):
        self.fixture = WorkerFixture(self)
        self.directory = self.fixture.sealed_job()
        self.source = self.directory / "input-manifest.tsv"
        self.raw = self.source.read_bytes()

    def test_envelope_preserves_native_candidate_bytes(self):
        rows, native, origin, digest, query = parse(self.raw, self.fixture.commit)
        self.assertEqual(native, b"".join(line for line in self.raw.splitlines(keepends=True)
                         if line.split(b"\t", 1)[0] not in (b"schema", b"origin_manifest", b"query_script_sha256")))
        self.assertEqual(origin, self.fixture.pair.origin_path)
        self.assertEqual(digest, self.fixture.pair.origin_hash)
        self.assertEqual(query, query_hash(self.fixture.repo, self.fixture.commit))
        self.assertEqual(rows["source_commit"], [self.fixture.commit])

    def test_malformed_envelope_cannot_select_another_candidate(self):
        lines = self.raw.splitlines(keepends=True)
        origin = next(line for line in lines if line.startswith(b"origin_manifest\t"))
        changes = [self.raw + lines[0], self.raw + b"unknown\tx\n", self.raw.replace(origin, b""),
                   self.raw.replace(b"queue-input.v1", b"queue-input.v2"),
                   self.raw.replace(origin, b"origin_manifest\t/../other\t" + b"0" * 64 + b"\n"),
                   self.raw.replace(b"query_script_sha256\t", b"query_script_sha256\twrong"),
                   self.raw + b"\0", b"x" * 65_537]
        for raw in changes:
            with self.subTest(raw_hash=hashlib.sha256(raw).hexdigest()):
                with self.assertRaises((ValueError, UnicodeError)): parse(raw, self.fixture.commit)
        with self.assertRaises(ValueError): parse(self.raw, COMMIT)

    def test_validate_captures_sealed_origin_and_rejects_wrong_query_blob(self):
        rows, native, docs, origin_hash, query = validate(self.source, self.fixture.commit, self.fixture.repo)
        self.assertEqual(docs[0].sha256, origin_hash)
        copied = self.directory / "retained-origin.json"
        self.assertEqual(hashlib.sha256(copied.read_bytes()).hexdigest(), origin_hash)
        self.assertEqual(copied.stat().st_mode & 0o777, 0o400)
        wrong = self.raw.replace(query.encode(), b"0" * 64)
        self.source.chmod(0o600); self.source.write_bytes(wrong)
        with self.assertRaises(ValueError): validate(self.source, self.fixture.commit, self.fixture.repo)

    def test_lineage_change_refuses_validation_and_seal_overwrite(self):
        with self.assertRaises(FileExistsError): seal(self.source, self.fixture.commit, self.directory, self.fixture.repo)
        path = self.fixture.pair.request_path
        original = path.read_bytes(); path.write_bytes(original + b" ")
        with self.assertRaises(ValueError): validate(self.source, self.fixture.commit, self.fixture.repo)
        with self.assertRaises(ValueError): bound(self.directory, self.fixture.repo, self.fixture.commit)

    def test_ledger_requires_exact_seven_immutable_fields(self):
        ledger = self.fixture.queue / "job-ledger/a-fixture/entry.env"
        original = ledger.read_bytes()
        for replacement in (original + b"unknown=x\n", original + b"job_id=a-fixture\n",
                            original.replace(b"origin_manifest_sha256=", b"origin_missing=")):
            ledger.chmod(0o600); ledger.write_bytes(replacement); ledger.chmod(0o400)
            with self.assertRaises(ValueError): job(self.directory, self.fixture.commit)
        ledger.chmod(0o600); ledger.write_bytes(original)
        with self.assertRaises(ValueError): job(self.directory, self.fixture.commit)

    def test_bound_hashes_queue_binary_and_current_query_source(self):
        bound(self.directory, self.fixture.repo, self.fixture.commit)
        path = self.directory / "hvf_gic_boot_probe"
        original = path.read_bytes(); path.chmod(0o600); path.write_bytes(b"changed")
        with self.assertRaises(ValueError): bound(self.directory, self.fixture.repo, self.fixture.commit)
        path.write_bytes(original); path.chmod(0o400)
        query = self.fixture.repo / SCRIPT; query.write_bytes(query.read_bytes() + b"# altered\r\n")
        with self.assertRaises(ValueError): bound(self.directory, self.fixture.repo, self.fixture.commit)

    def test_source_check_is_clean_fixed_git_despite_inherited_overrides(self):
        with patch.dict(os.environ, GIT_DIR=str(self.fixture.root / "nonexistent"), GIT_WORK_TREE="/never"):
            require_source(self.fixture.repo, self.fixture.commit)
            self.assertEqual(query_hash(self.fixture.repo, self.fixture.commit),
                             hashlib.sha256((self.fixture.repo / SCRIPT).read_bytes()).hexdigest())
        (self.fixture.repo / "unexpected").write_text("owned dirty source")
        with self.assertRaises(ValueError): require_source(self.fixture.repo, self.fixture.commit)

    def test_flat_receipt_rejects_promotional_or_private_additions(self):
        identity = job(self.directory, self.fixture.commit)
        base = empty(identity, "incomplete"); checked(base, identity)
        for key, value in (("pass", True), ("claim_eligible", True), ("criterion_pass", True),
                           ("worker_cleanup_verified", 1), ("fixed_volume_count", True),
                           ("private_path", "/never"), ("reason", [])):
            with self.subTest(key=key):
                changed = dict(base); changed[key] = value
                with self.assertRaises((ValueError, TypeError)): checked(changed, identity)
        self.assertFalse(any(str(self.fixture.root) in str(value) for value in base.values()))


if __name__ == "__main__": unittest.main()
