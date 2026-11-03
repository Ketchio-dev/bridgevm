#!/usr/bin/env python3
"""Literal-path process observation refuses matches and probe errors."""
from pathlib import Path
import os
import signal
import subprocess
import sys
import tempfile
import time
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "scripts/live-gates"))
import product_e2e_process_absent as probe


class ProcessAbsenceTests(unittest.TestCase):
    def test_probe_errors_and_malformed_output_are_not_absence(self):
        for code, output, expected in [(1, b"", True), (2, b"", False),
                                       (3, b"", False), (0, b"bad", False),
                                       (0, b"", False), (0, str(os.getpid()).encode(), True),
                                       (0, b"999999", False)]:
            def run(argv, **kwargs):
                return subprocess.CompletedProcess(argv, code, output)
            self.assertEqual(probe.absent("/owned/[a](b)+", run), expected)
        def timeout(argv, **kwargs):
            raise subprocess.TimeoutExpired(argv, 10)
        self.assertFalse(probe.absent("/owned", timeout))

    def test_literal_metacharacter_survivor_prevents_cleanup(self):
        with tempfile.TemporaryDirectory(prefix="process-[owned](x)+-") as directory:
            marker = str(Path(directory) / "lane-1")
            process = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(30)", marker])
            try:
                result = subprocess.run([sys.executable, probe.__file__, marker], timeout=15)
                self.assertEqual(result.returncode, 1)
            finally:
                process.terminate(); process.wait(timeout=10)
            result = subprocess.run([sys.executable, probe.__file__, marker], timeout=15)
            self.assertEqual(result.returncode, 0)

    def test_end_stops_only_processes_inside_the_tree(self):
        with tempfile.TemporaryDirectory(prefix="process-[owned](x)+-") as directory:
            work = str(Path(directory) / "bridgevm-e2e-job.abc123")
            sleep = "import signal, time; signal.signal(signal.SIGTERM, signal.SIG_IGN); time.sleep(30)"
            owned = subprocess.Popen([sys.executable, "-c", sleep, work + "/lane-1/library"], start_new_session=True)
            other = subprocess.Popen([sys.executable, "-c", sleep, work + "-other/lane-1"], start_new_session=True)
            try:
                time.sleep(0.3)
                self.assertTrue(probe.end(work + "/", term_seconds=1.0))
                self.assertEqual(owned.wait(timeout=5), -signal.SIGKILL)  # ignored TERM, then KILL
                self.assertIsNone(other.poll())
            finally:
                for process in (owned, other):
                    process.kill(); process.wait(timeout=10)


if __name__ == "__main__":
    unittest.main()
