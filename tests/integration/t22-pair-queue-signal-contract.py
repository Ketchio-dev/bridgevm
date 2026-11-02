#!/usr/bin/env python3
"""Adapter cancellation escapes subprocess wait OSError fallbacks; no preparer runs."""
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/live-gates"))
from t22_pair_queue_command import launch, command_hash
from t22_pair_queue_proof import adapter_context
from signal_driver_support import swallowing_wait, terminate_when_ready

DRIVER = """import pathlib, sys
sys.path.insert(0, sys.argv[1])
from t22_pair_queue_command import launch
child = [sys.executable, "-c", "import pathlib,sys,time; pathlib.Path(sys.argv[1]).touch(); time.sleep(30)", sys.argv[3]]
raise SystemExit(launch(child, pathlib.Path(sys.argv[2]), {"commit": "a" * 40, "job_id": "owned-fixture"}, timeout=60))
"""


class AdapterSignal(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="bridgevm-d10-signal-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.identity = {"commit": "a" * 40, "job_id": "owned-fixture"}
        self.handlers = {sig: signal.getsignal(sig) for sig in (signal.SIGTERM, signal.SIGINT)}

    def tearDown(self):
        self.assertEqual({sig: signal.getsignal(sig) for sig in self.handlers}, self.handlers)
        for path in self.root.rglob("*"): path.chmod(0o700 if path.is_dir() else 0o600)

    def test_handler_escapes_a_wait_that_swallows_oserror(self):
        for requested, cause in ((False, "interrupted"), (True, "canceled")):
            with self.subTest(cause=cause):
                directory = self.root / cause; directory.mkdir()
                if requested: (directory / "cancel.requested").touch()
                children, real = [], subprocess.Popen
                def spawn(*args, **kwargs):
                    child = real(*args, **kwargs); children.append(child)
                    wait, first = child.wait, True
                    def swallowing(*arguments, **options):
                        nonlocal first
                        if first:
                            first = False; swallowing_wait()
                            return 0  # The modeled helper reports completion.
                        return wait(*arguments, **options)
                    child.wait = swallowing
                    return child
                argv = [sys.executable, "-c", "import time; time.sleep(30)"]
                with patch("t22_pair_queue_command.subprocess.Popen", side_effect=spawn):
                    self.assertEqual(launch(argv, directory, self.identity), 1)
                value, _ = adapter_context(directory, self.identity, command_hash(argv))
                self.assertEqual(value["cause"], cause)
                self.assertIsNotNone(children[0].returncode)

    def test_real_term_reaches_owned_cleanup_within_five_seconds(self):
        # Unpiped wait(timeout): only CPython 3.15's macOS kqueue wait loses this on old handlers.
        directory, ready, driver = self.root / "term", self.root / "ready", self.root / "driver.py"
        directory.mkdir(); driver.write_text(DRIVER)
        (directory / "cancel.requested").touch()
        status = terminate_when_ready([driver, ROOT / "scripts/live-gates", directory, ready], ready)
        self.assertEqual(status, 1, "TERM did not end the adapter within five seconds")
        value = json.loads((directory / "adapter-launch-context.json").read_bytes())
        self.assertEqual(value["cause"], "canceled")
        self.assertTrue(value["terminal_observed"])
        with self.assertRaises(ProcessLookupError): os.kill(value["core_pid"], 0)


if __name__ == "__main__": unittest.main()
