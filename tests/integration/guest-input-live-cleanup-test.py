#!/usr/bin/env python3
"""Owned process lifecycle regression; does not start a VM."""
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "scripts/live-gates"))
import guest_input_live_cleanup as cleanup
from guest_input_cleanup_permission_tests import PermissionCleanup
from guest_input_cleanup_ownership_tests import OwnershipCleanup
from guest_input_launch_ownership_tests import LaunchOwnershipCleanup


class Cleanup(unittest.TestCase):
    def test_refuses_current_group(self):
        class Unowned:
            pid = os.getpgrp()
        with self.assertRaises(ValueError):
            cleanup.stop(Unowned())

    def test_cleanup_failure_preserves_incomplete_receipt(self):
        with tempfile.TemporaryDirectory() as tmp:
            receipt = {"source_integrity": False, "claim_eligible": False}
            with patch.object(cleanup, "stop", return_value=False), patch.object(cleanup, "digest") as digest:
                cleanup.finalize(receipt, object(), {}, {}, {}, Path(tmp))
                digest.assert_not_called()
            saved = json.loads((Path(tmp) / "receipt.json").read_text())
            self.assertFalse(saved["complete"])
            self.assertFalse(saved["cleanup_complete"])
            self.assertFalse(saved["claim_eligible"])

    def test_cleanup_exception_is_recorded(self):
        with tempfile.TemporaryDirectory() as tmp:
            receipt = {"source_integrity": False, "claim_eligible": False}
            with patch.object(cleanup, "stop", side_effect=PermissionError()):
                cleanup.finalize(receipt, object(), {}, {}, {}, Path(tmp))
            self.assertEqual(receipt["cleanup_failure_type"], "PermissionError")
            self.assertFalse(receipt["complete"])


if __name__ == "__main__":
    unittest.main()
