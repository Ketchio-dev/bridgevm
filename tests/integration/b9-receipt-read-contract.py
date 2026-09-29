#!/usr/bin/env python3
"""One-descriptor B9 receipt, queue ledger and retained log contracts."""

from __future__ import annotations

import hashlib
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

REPO = Path(__file__).resolve().parents[2]
MODULES = REPO / "scripts/live-gates"
sys.path.insert(0, str(MODULES))
import b9_ledger_bytes as ledger_bytes
import b9_share_asset_integrity as integrity
from b9_real_workload_inputs import KEYS
from b9_real_workload_receipt import TIER, job_fields, read_json, read_raw_run_log

FIFO_CHILD = """
import sys
from pathlib import Path
sys.path.insert(0, sys.argv[1])
from b9_real_workload_receipt import job_fields, read_json, read_raw_run_log
root, mode = Path(sys.argv[2]), sys.argv[3]
try:
    if mode == 'json': read_json(root/'receipt.json')
    elif mode in ('job', 'ledger'): job_fields(root/'queue'/'jobs'/'job-1')
    else: read_raw_run_log(root/'run.log', {'bytes': 2, 'sha256': '0'*64})
except (OSError, ValueError):
    sys.exit(0)
sys.exit(3)
"""


class ReceiptReadContract(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory(dir=str(Path(tempfile.gettempdir()).resolve()))
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name).resolve(strict=True)
        self.json_path = self.root / "receipt.json"
        self.json_path.write_bytes(b'{"x":1}')
        self.job = self.root / "queue" / "jobs" / "job-1"
        self.job.mkdir(parents=True)
        self.ledger = self.root / "queue" / "job-ledger" / "job-1" / "entry.env"
        self.ledger.parent.mkdir(parents=True)
        fields = {"job_id": "job-1", "tier": TIER, "commit": "a" * 40,
                  "input_manifest_sha256": "b" * 64, "sealed_binary_sha256": "c" * 64}
        fields.update({"asset_" + key + "_sha256": hashlib.sha256(key.encode()).hexdigest()
                       for key in KEYS})
        raw = "".join(f"{key}={value}\n" for key, value in sorted(fields.items())).encode()
        (self.job / "job.env").write_bytes(raw)
        self.ledger.write_bytes(raw)
        self.ledger.chmod(0o400)
        self.fields = fields

    def test_normal_json_job_ledger_and_log_bytes(self):
        self.assertEqual(read_json(self.json_path), {"x": 1})
        self.assertEqual(job_fields(self.job), self.fields)
        raw = b"B9 completed run log\n"
        log = self.root / "run.log"
        log.write_bytes(raw)
        seal = {"bytes": len(raw), "sha256": hashlib.sha256(raw).hexdigest()}
        self.assertEqual(read_raw_run_log(log, seal), raw)
        log.write_bytes(b"X" + raw[1:])
        with self.assertRaisesRegex(ValueError, "private artifact seal"):
            read_raw_run_log(log, seal)
        with self.assertRaisesRegex(ValueError, "private artifact seal"):
            read_raw_run_log(log, {**seal, "bytes": len(raw) - 1})

    def test_json_duplicate_key_and_same_length_inode_swap_refuse(self):
        self.json_path.write_bytes(b'{"x":1,"x":2}')
        with self.assertRaisesRegex(ValueError, "duplicate B9 receipt key"):
            read_json(self.json_path)
        self.json_path.write_bytes(b'{"x":1}')
        replacement = self.root / "replacement.json"
        replacement.write_bytes(b'{"x":2}')
        original_open = os.open

        def swap_after_open(path, flags, *args):
            fd = original_open(path, flags, *args)
            if Path(path) == self.json_path:
                os.replace(replacement, self.json_path)
            return fd

        with patch.object(integrity.os, "open", swap_after_open):
            with self.assertRaisesRegex(ValueError, "metadata changed while reading"):
                read_json(self.json_path)

    def test_writable_or_replaced_ledger_refuses(self):
        self.ledger.chmod(0o600)
        with self.assertRaisesRegex(ValueError, "ledger type, mode or size"):
            job_fields(self.job)
        self.ledger.chmod(0o400)
        replacement = self.root / "replacement.env"
        replacement.write_bytes(self.ledger.read_bytes())
        replacement.chmod(0o400)
        original_open = os.open

        def swap_ledger_after_open(path, flags, *args):
            fd = original_open(path, flags, *args)
            if Path(path) == self.ledger:
                os.replace(replacement, self.ledger)
            return fd

        with patch.object(ledger_bytes.os, "open", swap_ledger_after_open):
            with self.assertRaises(ValueError):
                job_fields(self.job)

    def test_all_four_fifo_reads_refuse_without_writer(self):
        paths = (("json", self.json_path), ("job", self.job / "job.env"),
                 ("ledger", self.ledger), ("log", self.root / "run.log"))
        for mode, path in paths:
            with self.subTest(mode=mode):
                if path.exists():
                    path.unlink()
                os.mkfifo(path)
                result = subprocess.run([sys.executable, "-c", FIFO_CHILD,
                                         str(MODULES), str(self.root), mode],
                                        stdin=subprocess.DEVNULL, capture_output=True,
                                        text=True, timeout=2)
                self.assertEqual(result.returncode, 0, result.stderr)
                path.unlink()
                if mode == "job":
                    path.write_bytes(self.ledger.read_bytes())
                if mode == "ledger":
                    path.write_bytes((self.job / "job.env").read_bytes())
                    path.chmod(0o400)


if __name__ == "__main__":
    unittest.main()
