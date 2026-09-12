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


class Cleanup(unittest.TestCase):
    def test_exited_leader_with_term_ignoring_child(self):
        with tempfile.TemporaryDirectory() as tmp:
            ready = Path(tmp) / "ready"
            child = ("import os,signal,time;from pathlib import Path;"
                     "signal.signal(signal.SIGTERM,signal.SIG_IGN);"
                     "Path(" + repr(str(ready)) + ").write_text(str(os.getpid()));time.sleep(15)")
            parent = ("import subprocess,sys,time;from pathlib import Path;"
                      "subprocess.Popen([sys.executable,'-c'," + repr(child) + "]);"
                      "deadline=time.monotonic()+3\nwhile not Path(" + repr(str(ready))
                      + ").exists() and time.monotonic()<deadline:time.sleep(.01)")
            process = subprocess.Popen([sys.executable, "-c", parent], start_new_session=True)
            try:
                process.wait(timeout=5)
                self.assertTrue(ready.is_file())
                self.assertTrue(cleanup.group_alive(process.pid))
                self.assertTrue(cleanup.stop(process, grace=2))
                self.assertFalse(cleanup.group_alive(process.pid))
            finally:
                try:
                    os.killpg(process.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                process.wait(timeout=5)

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
