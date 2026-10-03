#!/usr/bin/env python3
"""D10 archive admission precedes its imports without changing unrelated history."""
import shutil
import subprocess
import unittest

from t22_pair_cache_fixtures import CacheFixture
from t22_pair_worker_fixtures import WorkerFixture, LIVE


class ArchiveAdmission(CacheFixture):
    def setUp(self):
        self.fixture = WorkerFixture(self)
        self.cli = self.fixture.repo / "scripts/live-gates/receipt-cli"
        shutil.copy2(LIVE / "bridgevm-live", self.cli)
        self.fixture.git("add", "scripts/live-gates/receipt-cli")
        self.fixture.git("-c", "user.name=Owned Fixture", "-c", "user.email=fixture@example.invalid",
                         "commit", "-qm", "owned actual archive reader")
        self.fixture.commit = self.fixture.git("rev-parse", "HEAD").stdout.strip()
        self.directory = self.fixture.sealed_job(state="running")
        self.worktree = self.fixture.repo
        self.marker = self.fixture.root / "cached-code-executed"
        (self.directory / "receipt.public.json").write_text('{"unverified":true}\n')

    def receipt(self):
        return subprocess.run([str(self.cli), "receipt", "a-fixture"], env=self.fixture.env(),
                              capture_output=True, text=True, timeout=35)

    def refused(self):
        result = self.receipt()
        self.assertNotEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, "")
        self.assertFalse(self.marker.exists(), "cached D10 code ran before archive admission")
        self.assertTrue(self.directory.is_dir())

    def tier(self, path, value):
        path.chmod(0o600)
        rows = path.read_text().splitlines()
        path.write_text("\n".join("tier=" + value if line.startswith("tier=") else line for line in rows) + "\n")
        path.chmod(0o400)

    def ledger(self): return self.fixture.queue / "job-ledger/a-fixture/entry.env"

    def test_existing_timestamp_cache_cannot_accept_invalid_receipt(self):
        self.cache("scripts/live-gates/t22_pair_queue_archive.py")
        self.refused()

    def test_public_d10_hint_cannot_hide_behind_legacy_seals(self):
        self.tier(self.directory / "job.env", "t1-vtimer"); self.tier(self.ledger(), "t1-vtimer")
        (self.directory / "receipt.public.json").write_text('{"tier":"d10-t22-owned-pair-preparation"}\n')
        self.cache("scripts/live-gates/t22_pair_queue_archive.py")
        self.refused()

    def test_ledger_d10_hint_cannot_hide_behind_legacy_job(self):
        self.tier(self.directory / "job.env", "t1-vtimer")
        self.cache("scripts/live-gates/t22_pair_queue_archive.py")
        self.refused()

    def test_duplicate_public_tier_cannot_hide_d10_from_admission(self):
        self.tier(self.directory / "job.env", "t1-vtimer"); self.tier(self.ledger(), "t1-vtimer")
        (self.directory / "receipt.public.json").write_text('{"tier":"d10-t22-owned-pair-preparation","tier":"t1-vtimer"}\n')
        self.cache("scripts/live-gates/t22_pair_queue_archive.py")
        self.refused()

    def test_dirty_d10_import_cannot_accept_invalid_receipt(self):
        path = self.worktree / "scripts/live-gates/t22_pair_queue_archive.py"
        path.write_text(f"from pathlib import Path\nPath({str(self.marker)!r}).write_text('dirty')\nraise SystemExit(0)\n" + path.read_text())
        self.refused()

    def test_unrelated_history_keeps_exact_bytes_and_ignores_unused_d10_cache(self):
        self.tier(self.directory / "job.env", "t1-vtimer"); self.tier(self.ledger(), "t1-vtimer")
        for path in (self.directory / "job.env", self.ledger()):
            path.chmod(0o600); path.write_text(path.read_text().replace(self.fixture.commit, "1234567")); path.chmod(0o400)
        public = self.directory / "receipt.public.json"; data = b'{"legacy": true}\n'; public.write_bytes(data)
        self.cache("scripts/live-gates/t22_pair_queue_archive.py")
        result = self.receipt()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.encode(), data)
        self.assertFalse(self.marker.exists())

    def test_d10_reader_requires_matching_source_commit(self):
        for path in (self.directory / "job.env", self.ledger()):
            path.chmod(0o600); path.write_text(path.read_text().replace(self.fixture.commit, "0" * 40)); path.chmod(0o400)
        self.refused()


if __name__ == "__main__": unittest.main()
