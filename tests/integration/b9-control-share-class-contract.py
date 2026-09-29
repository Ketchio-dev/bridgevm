#!/usr/bin/env python3
"""Synthetic B9 shared-control failure identity across runner stages."""

from __future__ import annotations

import hashlib
import os
from pathlib import Path
import runpy
import subprocess
import sys
import tempfile
import unittest

REPO = Path(__file__).resolve().parents[2]
MODULES = REPO / "scripts/live-gates"
sys.path.insert(0, str(MODULES))
from b9_asset_error import AssetIntegrityError
from b9_failure_classification import classify_failure

CONTROL = runpy.run_path(str(MODULES / "run-b9-real-workload-pilot.py"))["control_command"]
FIFO_CHILD = """
import runpy, sys
from pathlib import Path
sys.path.insert(0, sys.argv[1])
from b9_asset_error import AssetIntegrityError
control = runpy.run_path(str(Path(sys.argv[1])/'run-b9-real-workload-pilot.py'))['control_command']
try:
    control(Path(sys.argv[2]), '0'*64, 'Firstboot')
except AssetIntegrityError:
    sys.exit(0)
sys.exit(3)
"""


class ControlShareClassContract(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory(dir=str(Path(tempfile.gettempdir()).resolve()))
        self.addCleanup(temp.cleanup)
        self.share = Path(temp.name).resolve(strict=True)
        self.path = self.share / "bv-b9-control.ps1"
        self.raw = b"# synthetic control\r\n"
        self.digest = hashlib.sha256(self.raw).hexdigest()
        self.path.write_bytes(self.raw)
        self.receipt = {"result_class": "GUEST_NOT_READY"}

    def test_exact_control_positive_and_changed_then_restored_invalid(self):
        command = CONTROL(self.share, self.digest, "Firstboot")
        self.assertIn("-ExpectedControlSha256 " + self.digest, command)
        self.path.write_bytes(b"# changed control\r\n")
        with self.assertRaisesRegex(AssetIntegrityError, "differ from sealed source") as changed:
            CONTROL(self.share, self.digest, "Firstboot")
        self.path.write_bytes(self.raw)
        self.assertIn("-Action Firstboot", CONTROL(self.share, self.digest, "Firstboot"))
        for stage in ("boot", "shutdown"):
            self.assertEqual(classify_failure(stage, changed.exception, self.receipt),
                             "INVALID_EVIDENCE")

    def test_missing_and_symlink_control_are_asset_errors(self):
        self.path.unlink()
        with self.assertRaisesRegex(AssetIntegrityError, "cannot be verified") as missing:
            CONTROL(self.share, self.digest, "Firstboot")
        self.assertIsInstance(missing.exception.__cause__, OSError)
        source = self.share / "other.ps1"
        source.write_bytes(self.raw)
        self.path.symlink_to(source)
        with self.assertRaisesRegex(AssetIntegrityError, "cannot be verified") as alias:
            CONTROL(self.share, self.digest, "Shutdown")
        self.assertIsInstance(alias.exception.__cause__, ValueError)
        for error in (missing.exception, alias.exception):
            for stage in ("boot", "shutdown"):
                self.assertEqual(classify_failure(stage, error, self.receipt),
                                 "INVALID_EVIDENCE")

    def test_no_writer_fifo_refuses_within_child_timeout(self):
        self.path.unlink()
        os.mkfifo(self.path)
        completed = subprocess.run([sys.executable, "-c", FIFO_CHILD, str(MODULES),
                                    str(self.share)], stdin=subprocess.DEVNULL,
                                   capture_output=True, text=True, timeout=2)
        self.assertEqual(completed.returncode, 0, completed.stderr)

    def test_unrelated_boot_and_shutdown_keep_original_class(self):
        self.assertEqual(classify_failure("boot", ValueError("unrelated"), self.receipt),
                         "GUEST_NOT_READY")
        prior = {"result_class": "VLC_PID_PRESENTS_CAPTURED"}
        self.assertEqual(classify_failure("shutdown", TimeoutError("VM stalled"), prior),
                         "PLAYBACK_INCOMPLETE")


if __name__ == "__main__":
    unittest.main()
