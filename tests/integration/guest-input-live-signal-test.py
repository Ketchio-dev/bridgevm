#!/usr/bin/env python3
"""D5 cancellation escapes subprocess wait OSError fallbacks; does not start a VM."""
from contextlib import redirect_stdout
import io
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
from guest_input_launch_fixture import module, run_case
from signal_driver_support import swallowing_wait, terminate_when_ready

DRIVER = """import importlib.util, json, pathlib, signal, sys
gates, base = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
sys.path.insert(0, str(gates))
spec = importlib.util.spec_from_file_location("runner", gates / "run-guest-input-live.py")
runner = importlib.util.module_from_spec(spec); spec.loader.exec_module(runner)
from guest_input_driver_variant import ProductionController
for sig in (signal.SIGTERM, signal.SIGINT): signal.signal(sig, runner.interrupted)
controller = ProductionController(base / "agent.ctl", base / "run.log", base / "share", base / "fake-driver")
try:
    controller.dispatch_inputs(1, 2); outcome = "completed"
except BaseException as error:
    outcome = type(error).__name__
(base / "result.json").write_text(json.dumps(outcome))
"""
FAKE = "#!{}\nimport os,pathlib,time\npathlib.Path({!r}).write_text(str(os.getpid()))\ntime.sleep(30)\n"


class LiveSignal(unittest.TestCase):
    def test_codesign_wait_cannot_swallow_cancellation(self):
        def codesign(command, **options):
            swallowing_wait()
            return subprocess.CompletedProcess(command, 0)
        with tempfile.TemporaryDirectory() as tmp:
            value = run_case(Path(tmp), "d5", codesign, preflight=True)
        self.assertIsNone(value["caught"])
        self.assertEqual(value["spawned"], 0)
        self.assertEqual(value["receipt"]["failure_type"], "LiveDiagnosticInterrupted")
        self.assertTrue(value["receipt"]["cleanup_complete"])

    def test_entry_reports_cancellation_outside_the_owned_try(self):
        runner = module("run-guest-input-live.py")
        argv = ["run-guest-input-live.py", "--commit", "a" * 40, "--job-id", "control", "--input-manifest", "unused"]
        printed = io.StringIO()
        with patch.object(runner, "execute", side_effect=lambda _: runner.interrupted(signal.SIGTERM, None)), \
                patch.object(sys, "argv", argv), redirect_stdout(printed):
            self.assertEqual(runner.main(), 1)
        self.assertEqual(json.loads(printed.getvalue()), {"claim_eligible": False, "complete": False,
                                                          "failure_type": "LiveDiagnosticInterrupted"})

    def test_real_term_stops_the_piped_production_driver(self):
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp).resolve(); ready, fake = base / "fake.pid", base / "fake-driver"
            fake.write_text(FAKE.format(sys.executable, str(ready))); fake.chmod(0o700)
            (base / "driver.py").write_text(DRIVER)
            status = terminate_when_ready([base / "driver.py", ROOT / "scripts/live-gates", base], ready)
            self.assertEqual(status, 0, "TERM did not end the driver dispatch within five seconds")
            self.assertEqual(json.loads((base / "result.json").read_text()), "LiveDiagnosticInterrupted")
            with self.assertRaises(ProcessLookupError): os.kill(int(ready.read_text()), 0)


if __name__ == "__main__":
    unittest.main()
