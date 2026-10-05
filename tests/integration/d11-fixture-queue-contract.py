#!/usr/bin/env python3
"""Actual queue CLI on an owned synthetic repository/queue; never runs a tier."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

from d11_fixture_test_support import ROOT, rows, manifest
from d11_fixture_files import digest, TreeSeal, record
from d11_fixture_inputs import RESOURCES, SEED, POLICY
from d11_fixture_receipt import empty


class Queue(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temp = tempfile.TemporaryDirectory(); cls.base = Path(cls.temp.name).resolve()
        cls.repo = cls.base / "repo"; cls.repo.mkdir()
        (cls.repo / "scripts/live-gates").mkdir(parents=True)
        for path in (ROOT / "scripts").glob("*.py"):
            shutil.copy2(path, cls.repo / "scripts" / path.name)
        for path in (ROOT / "scripts/live-gates").iterdir():
            if path.is_file() and (path.suffix in (".py", ".sh") or path.name == "bridgevm-live"):
                shutil.copy2(path, cls.repo / "scripts/live-gates" / path.name)
        env = dict(os.environ, GIT_CONFIG_GLOBAL="/dev/null", GIT_CONFIG_SYSTEM="/dev/null")
        def git(*args):
            return subprocess.check_output(["git", "-C", str(cls.repo), *args], env=env, stderr=subprocess.DEVNULL).decode().strip()
        git("init", "-q"); git("add", ".")
        git("-c", "user.name=Synthetic", "-c", "user.email=synthetic@example.invalid", "commit", "-qm", "synthetic D11 queue")
        cls.commit = git("rev-parse", "HEAD")
        cls.home = cls.base / "home"; (cls.home / "BridgeVM").mkdir(parents=True)
        cls.queue = cls.home / "BridgeVM/live-queue"
        cls.assets = cls.base / "assets"; cls.assets.mkdir()
        value = rows(cls.assets); value["source_commit"] = [cls.commit]
        for key in ("iso", "payload_manifest", "binary", "renderer"):
            path = cls.assets / key; path.write_bytes(("synthetic-" + key).encode())
            value[key][1] = digest(path)
        firmware = cls.assets / "firmware"
        shutil.copy2(ROOT / "crates/bridgevm-hvf/firmware/edk2-aarch64-secure-code.fd", firmware)
        for role in ("payload", "tools", "fixture_helper"): (cls.assets / role).mkdir()
        (cls.assets / "payload/driver").write_bytes(b"synthetic payload")
        for name in ("wimlib-imagex", "bridgevm-catalog-verify", "bv-file-compare.exe"):
            (cls.assets / "tools" / name).write_bytes(b"never executed")
        helper = cls.assets / "fixture_helper"
        (helper / "d11-fixture-helper").write_bytes(b"never executed")
        (helper / "source-commit.txt").write_text(cls.commit + "\n")
        (helper / RESOURCES).mkdir(parents=True)
        for name in (SEED, POLICY): (helper / RESOURCES / name).write_bytes(b"synthetic resource")
        for role in ("payload", "tools", "fixture_helper"):
            with TreeSeal(cls.assets / role) as seal: value[role][1] = seal.sha256
        cls.source = cls.base / "manifest.tsv"; cls.source.write_bytes(manifest(value))
        cls.env = dict(os.environ, HOME=str(cls.home), BRIDGEVM_LIVE_ROOT=str(cls.queue), PYTHONDONTWRITEBYTECODE="1")
        cls.cli = cls.repo / "scripts/live-gates/bridgevm-live"

    @classmethod
    def tearDownClass(cls): cls.temp.cleanup()

    def command(self, *args):
        return subprocess.run(["/bin/bash", str(self.cli), *args], env=self.env, capture_output=True, text=True, timeout=30)

    def submit(self, name):
        result = self.command("submit", "d11-native-fixture-preparation", "--sha", self.commit,
                              "--input-manifest", str(self.source), "--job-id", name)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.strip(), name)
        return self.queue / "queued" / name

    def test_sealed_submission_and_duplicate_id_refusal(self):
        job = self.submit("d11-submit")
        ledger = self.queue / "job-ledger/d11-submit/entry.env"
        self.assertEqual(ledger.stat().st_mode & 0o222, 0)
        self.assertEqual((job / "input-manifest.tsv").read_bytes(), self.source.read_bytes())
        result = self.command("submit", "d11-native-fixture-preparation", "--sha", self.commit,
                              "--input-manifest", str(self.source), "--job-id", "d11-submit")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(len(list((self.queue / "queued").glob("d11-submit"))), 1)

    def test_archive_redaction_and_worker_partial_cleanup_fence(self):
        path = self.submit("d11-receipt")
        binding = dict(line.split("=", 1) for line in (self.queue / "job-ledger/d11-receipt/entry.env").read_text().splitlines())
        value = empty(binding, "clean-refusal"); value["worker_cleanup_verified"] = True
        record(path / "receipt.json", value); record(path / "receipt.public.json", value)
        destination = self.queue / "done/d11-receipt"; path.rename(destination)
        result = self.command("receipt", "d11-receipt")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout), value)
        script = self.repo / "scripts/live-gates/t17-worker-cleanup-fence.sh"
        command = ["/bin/bash", "--noprofile", "--norc", "-p", "-c",
                   'source "$1"; bridgevm_t17_guard_or_fence "$2" "$3" "$4" "$5" "$6" "$7"',
                   "synthetic", str(script), "d11-native-fixture-preparation", str(destination),
                   str(self.repo), self.commit, "d11-receipt", str(self.queue)]
        self.assertEqual(subprocess.run(command, env=self.env, capture_output=True, timeout=30).returncode, 0)
        output = self.home / "BridgeVM/d11-fixtures/d11-receipt"; output.mkdir(parents=True)
        refused = subprocess.run(command, env=self.env, capture_output=True, timeout=30)
        self.assertEqual(refused.returncode, 126)
        self.assertTrue((self.queue / "worker-cleanup-required").is_file())
        public = destination / "receipt.public.json"; public.chmod(0o600)
        public.write_text(json.dumps({**value, "password": "synthetic-not-a-credential"}))
        refused = self.command("receipt", "d11-receipt")
        self.assertNotEqual(refused.returncode, 0); self.assertEqual(refused.stdout, "")


if __name__ == "__main__": unittest.main()
