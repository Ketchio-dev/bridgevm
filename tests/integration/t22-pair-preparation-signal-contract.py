#!/usr/bin/env python3
"""Owned pair cancellation escapes subprocess wait OSError fallbacks; no VM boots."""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

CONTRACT = Path(__file__).resolve().with_name("t22-pair-preparation-contract.py")
spec = importlib.util.spec_from_file_location("prepare_fixture", CONTRACT)
fixture = importlib.util.module_from_spec(spec); spec.loader.exec_module(fixture)
from guest_input_group_liveness import group_alive
from signal_driver_support import swallowing_wait, terminate_when_ready

REAL_RUN, REAL_POPEN = subprocess.run, subprocess.Popen
DRIVER = """import importlib.util, json, pathlib, subprocess, sys
from unittest.mock import patch
sys.path.insert(0, str(pathlib.Path(sys.argv[1]).parent))
spec = importlib.util.spec_from_file_location("prepare_fixture", sys.argv[1])
fixture = importlib.util.module_from_spec(spec); spec.loader.exec_module(fixture)
ready, result, real = pathlib.Path(sys.argv[2]), pathlib.Path(sys.argv[3]), subprocess.check_output
def piped(stack):
    pinned = subprocess.check_output
    def output(command, **options):
        if "t22-firmware" not in command: return pinned(command, **options)
        body = "import os,pathlib,sys,time; pathlib.Path(sys.argv[1]).write_text(str(os.getpid())); time.sleep(30)"
        return real([sys.executable, "-c", body, str(ready)], text=True, timeout=30)
    stack.enter_context(patch("subprocess.check_output", side_effect=output))
owned = fixture.Preparation(); owned.setUp()
try:
    code = owned.run_preparer(more=piped)
    result.write_text(json.dumps({"code": code, "receipt": owned.receipt()}))
finally:
    owned.tearDown()
"""


class PreparationSignal(unittest.TestCase):
    def setUp(self):
        self.owned = fixture.Preparation(); self.owned.setUp()

    def tearDown(self): self.owned.tearDown()

    def refused(self, more):
        self.assertEqual(self.owned.run_preparer(more=more), 1)
        receipt = self.owned.receipt()
        self.assertEqual(receipt["failure_type"], "PairPreparationInterrupted")
        self.assertFalse(receipt["preparation_complete"])
        self.assertTrue(receipt["cleanup_complete"])
        self.assertFalse((self.owned.output / "t22-input-manifest.tsv").exists())
        return receipt

    def test_codesign_wait_cannot_swallow_cancellation(self):
        def run(command, **kwargs):
            if command[0] != "/usr/bin/codesign": return REAL_RUN(command, **kwargs)
            swallowing_wait()
            return subprocess.CompletedProcess(command, 0)
        receipt = self.refused(lambda stack: stack.enter_context(patch("subprocess.run", side_effect=run)))
        self.assertNotIn("owned_launch_attempt_sha256", receipt)

    def test_shutdown_wait_cannot_swallow_cancellation(self):
        children = []
        def capture(command, **kwargs):
            process = REAL_POPEN(command, **kwargs)
            if kwargs.get("start_new_session"):
                children.append(process); wait, first = process.wait, True
                def swallowing(*args, **options):
                    nonlocal first
                    if first: first = False; swallowing_wait()
                    return wait(*args, **options)
                process.wait = swallowing
            return process
        receipt = self.refused(lambda stack: stack.enter_context(
            patch("t22_pair_runtime.subprocess.Popen", side_effect=capture)))
        self.assertFalse(receipt["natural_shutdown_observed"])
        self.assertEqual(len(children), 1); self.assertFalse(group_alive(children[0].pid))

    def test_real_term_interrupts_the_piped_firmware_policy_probe(self):
        # Piped check_output(timeout): every CPython's PollSelector swallowed the old handler.
        with tempfile.TemporaryDirectory() as temporary:
            base = Path(temporary).resolve(); ready, result = base / "probe.pid", base / "result.json"
            (base / "driver.py").write_text(DRIVER); (base / "tmp").mkdir(mode=0o700)
            status = terminate_when_ready([base / "driver.py", CONTRACT, ready, result], ready,
                                          env=dict(os.environ, TMPDIR=str(base / "tmp")))
            self.assertEqual(status, 0, "TERM did not end preparation within five seconds")
            value = json.loads(result.read_text())
            self.assertEqual(value["code"], 1)
            self.assertEqual(value["receipt"]["failure_type"], "PairPreparationInterrupted")
            self.assertTrue(value["receipt"]["cleanup_complete"])
            with self.assertRaises(ProcessLookupError): os.kill(int(ready.read_text()), 0)


if __name__ == "__main__": unittest.main()
