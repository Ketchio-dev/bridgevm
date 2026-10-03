#!/usr/bin/env python3
"""Actual copied submission seals D10 without installing or running its worker."""
import hashlib
from pathlib import Path
import shutil
import subprocess
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "scripts/live-gates"))

from t22_pair_worker_fixtures import WorkerFixture, LIVE, TIER
from t22_pair_queue_inputs import bound, job


class Submission(unittest.TestCase):
    def setUp(self):
        self.fixture = WorkerFixture(self)
        self.cli = self.fixture.repo / "scripts/live-gates/bridgevm-live"
        shutil.copy2(LIVE / "bridgevm-live", self.cli)
        self.fixture.git("add", "-A")
        self.fixture.git("-c", "user.name=Owned Fixture", "-c", "user.email=fixture@example.invalid", "commit", "-qm", "actual submit fixture")
        self.fixture.commit = self.fixture.git("rev-parse", "HEAD").stdout.strip()
        self.source = self.fixture.sealed_job(name="source-envelope") / "input-manifest.tsv"

    def submit(self, name, source=True):
        command = ["/bin/bash", str(self.cli), "submit", TIER, "--sha", self.fixture.commit, "--job-id", name]
        if source: command.extend(("--input-manifest", str(self.source)))
        return subprocess.run(command, env=self.fixture.env(), capture_output=True, text=True, timeout=20)

    def test_actual_submit_retains_origin_query_and_binary_seals(self):
        result = self.submit("actual-sealed")
        self.assertEqual(result.returncode, 0, result.stderr)
        directory = self.fixture.queue / "queued/actual-sealed"
        identity, _, _, docs = bound(directory, self.fixture.repo, self.fixture.commit)
        self.assertEqual(len(identity), 7)
        self.assertEqual(identity["input_manifest_sha256"], hashlib.sha256(self.source.read_bytes()).hexdigest())
        self.assertEqual(docs[0].sha256, identity["origin_manifest_sha256"])
        self.assertFalse((self.fixture.queue / "running/actual-sealed").exists())
        self.assertFalse((self.fixture.home / "BridgeVM/t22-prepared-pairs").exists())
        again = self.submit("actual-sealed")
        self.assertNotEqual(again.returncode, 0)
        self.assertIn("burned", again.stderr)
        job(directory, self.fixture.commit)

    def test_missing_envelope_and_too_long_job_id_refuse_before_enqueue(self):
        for name, present in (("no-input", False), ("x" * 97, True)):
            with self.subTest(name=name):
                result = self.submit(name, present)
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse((self.fixture.queue / "queued" / name).exists())
                self.assertFalse((self.fixture.queue / "job-ledger" / name).exists())

    def test_altered_query_seal_refuses_without_allocating_output_or_job(self):
        self.source.chmod(0o600)
        raw = self.source.read_bytes()
        line = next(value for value in raw.splitlines() if value.startswith(b"query_script_sha256\t"))
        self.source.write_bytes(raw.replace(line, b"query_script_sha256\t" + b"0" * 64))
        result = self.submit("wrong-query")
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse((self.fixture.queue / "job-ledger/wrong-query").exists())
        self.assertFalse((self.fixture.queue / "queued/wrong-query").exists())


if __name__ == "__main__": unittest.main()
