#!/usr/bin/env python3
"""Synthetic T20 failed-run cleanup, publication, and worker-fence contracts."""
from __future__ import annotations

import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/live-gates"))
from native_snapshot_restore_receipt import TIER, initial

COMMIT = subprocess.check_output(["git", "-C", str(ROOT), "rev-parse", "HEAD"], text=True).strip()
MANIFEST = "b" * 64
BINARY = "c" * 64


def runner_module():
    path = ROOT / "scripts/live-gates/run-native-snapshot-restore-tier.py"
    spec = importlib.util.spec_from_file_location("native_t20_runner", path)
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


def failed_job(root: Path) -> Path:
    job_id = "synthetic-failed-t20"
    job = root / "queue/done" / job_id
    ledger = root / "queue/job-ledger" / job_id
    job.mkdir(parents=True)
    ledger.mkdir(parents=True)
    rows = {"job_id": job_id, "tier": TIER, "commit": COMMIT,
            "input_manifest_sha256": MANIFEST, "sealed_binary_sha256": BINARY}
    content = "".join(f"{key}={value}\n" for key, value in rows.items())
    (job / "job.env").write_text(content + "submitted_at=synthetic\n", encoding="utf-8")
    entry = ledger / "entry.env"
    entry.write_text(content, encoding="utf-8")
    entry.chmod(0o400)
    receipt = initial(job_id, COMMIT)
    receipt.update({"started_at": "2026-09-28T00:00:00+00:00",
                    "finished_at": "2026-09-28T00:00:01+00:00",
                    "input_manifest_sha256": MANIFEST, "binary_hash": BINARY,
                    "worker_cleanup_verified": True})
    (job / "receipt.json").write_text(json.dumps(receipt), encoding="utf-8")
    return job


def publish(job: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["bash", str(ROOT / "scripts/live-gates/publish-receipt.sh"),
         TIER, str(job), str(ROOT), COMMIT], capture_output=True, text=True, timeout=10,
    )


def guard(job: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["bash", "-c", 'source "$1"; bridgevm_t17_guard_or_fence "$2" "$3" "$4" "$5" "$6" "$7"',
         "_", str(ROOT / "scripts/live-gates/t17-worker-cleanup-fence.sh"),
         TIER, str(job), str(ROOT), COMMIT, job.name, str(job.parent.parent)],
        capture_output=True, text=True, timeout=10,
    )


class NativeSnapshotRestoreCleanupContract(unittest.TestCase):
    def test_failed_clean_job_publishes_and_worker_continues(self):
        with tempfile.TemporaryDirectory() as temporary:
            job = failed_job(Path(temporary))
            result = publish(job)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertFalse(json.loads((job / "receipt.public.json").read_text())["pass"])
            result = guard(job)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertFalse((job.parent.parent / "worker-cleanup-required").exists())

    def test_dirty_media_fences_queue_and_withholds_public_receipt(self):
        for name in ("prepared-inputs", "live"):
            with self.subTest(name=name), tempfile.TemporaryDirectory() as temporary:
                job = failed_job(Path(temporary))
                if name == "live":
                    (job / name).symlink_to(job / "missing-target")
                else:
                    (job / name).mkdir()
                self.assertNotEqual(publish(job).returncode, 0)
                self.assertFalse((job / "receipt.public.json").exists())
                self.assertEqual(guard(job).returncode, 126)
                self.assertTrue((job.parent.parent / "worker-cleanup-required").is_file())

    def test_runner_records_cleanup_on_failed_lifecycle(self):
        for dirty in (False, True):
            with self.subTest(dirty=dirty), tempfile.TemporaryDirectory() as temporary:
                output = Path(temporary) / "job"
                output.mkdir()
                runner = runner_module()
                def prepare(*_args):
                    (output / "prepared-inputs").mkdir()
                    return ({"input_manifest_sha256": MANIFEST, "binary_hash": BINARY},
                            {"binary": "synthetic-binary", "app_cli": "synthetic-cli"})
                def fail_lifecycle(*_args, **_kwargs):
                    if dirty:
                        (output / "live").mkdir()
                    return SimpleNamespace(returncode=1)
                def identity(command, **_kwargs):
                    return COMMIT if command[0] == "git" else "Mac17,9"
                with (mock.patch.object(runner.sys, "argv", ["runner", str(output), "job", "manifest", "binary"]),
                      mock.patch.object(runner.subprocess, "check_output", side_effect=identity),
                      mock.patch.object(runner.subprocess, "run", side_effect=fail_lifecycle),
                      mock.patch.object(runner.platform, "mac_ver", return_value=("26.0", (), "")),
                      mock.patch.object(runner, "sealed_hashes", return_value={
                          "input_manifest_sha256": MANIFEST, "binary_hash": BINARY}),
                      mock.patch.object(runner, "prepare", side_effect=prepare),
                      mock.patch.object(runner, "digest", return_value="d" * 64),
                      mock.patch.object(runner, "shutdown_count", return_value=0),
                      mock.patch.object(runner, "reauthenticate")):
                    self.assertEqual(runner.main(), 1)
                receipt = json.loads((output / "receipt.json").read_text(encoding="utf-8"))
                self.assertFalse(receipt["pass"])
                self.assertEqual((receipt["sample_count"], receipt["run_count"]), (0, 0))
                self.assertIs(receipt["worker_cleanup_verified"], not dirty)
                self.assertFalse((output / "prepared-inputs").exists())


if __name__ == "__main__":
    unittest.main()
