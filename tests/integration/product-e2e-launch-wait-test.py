#!/usr/bin/env python3
"""A helper that exits before open attaches its wait is still authenticated by the tier."""
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
WAIT_ERROR = "Unable to block on applications (initial call to kevent() failed: No such process)"


class LaunchWaitTests(unittest.TestCase):
    def test_only_completed_wait_race_is_recoverable(self):
        with tempfile.TemporaryDirectory(prefix="launch-wait.") as directory:
            root = Path(directory)
            helper = root / "Helper.app/Contents/MacOS/Helper"
            helper.parent.mkdir(parents=True)
            helper.touch()
            request, result = root / "request.json", root / "result.json"
            request.write_text("{}\n")
            # The launcher still invokes open once with the exact bundle and args.
            fake_open = root / "open"
            fake_open.write_text('#!/bin/bash\nprintf "%s\\n" "$*" >> "$CALLS"\nprintf "%s\\n" "$ERROR" >&2\nexit "$STATUS"\n')
            fake_open.chmod(0o755)
            calls = root / "calls"
            env = dict(os.environ, PATH=f"{root}:{os.environ['PATH']}", CALLS=str(calls))

            def launch(error=WAIT_ERROR, status=1):
                calls.write_text("")
                completed = subprocess.run(["bash", str(ROOT / "scripts/live-gates/launch-product-e2e-helper.sh"),
                                            str(helper), str(root / "helper.log"), str(request), str(result)],
                                           env=dict(env, ERROR=error, STATUS=str(status)), capture_output=True, text=True)
                self.assertEqual(len(calls.read_text().splitlines()), 1, "never relaunch an operation")
                self.assertIn(f"-W -n {helper.parents[2]}", calls.read_text())
                return completed.returncode

            self.assertNotEqual(launch(), 0, "missing result must fail")
            result.write_text("{}\n")
            self.assertEqual(launch(), 0, "already exited helper's result goes to tier authentication")
            self.assertNotEqual(launch("The application cannot be opened"), 0)
            self.assertEqual(launch("", 0), 0)
            result.unlink()
            result.symlink_to(request)
            self.assertNotEqual(launch(), 0, "symlink is not a result")
            result.unlink()
            result.write_text("{}\n")
            survivor = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(60)", str(request)])
            try:
                self.assertNotEqual(launch(), 0, "a result does not prove helper termination")
            finally:
                survivor.terminate()
                survivor.wait(timeout=10)


if __name__ == "__main__":
    unittest.main()
