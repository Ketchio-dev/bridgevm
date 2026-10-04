#!/usr/bin/env python3
"""Real owned children remain bounded when wait or pathname probes fail."""
import importlib.util
from pathlib import Path
import signal
import subprocess
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("prepare_fixture", Path(__file__).with_name("t22-pair-preparation-contract.py"))
fixture = importlib.util.module_from_spec(spec); spec.loader.exec_module(fixture)
from guest_input_group_liveness import group_alive
from retained_windows_identity import directory_identity


class ProcessBoundary(unittest.TestCase):
    def setUp(self):
        self.owned = fixture.Preparation(); self.owned.setUp()

    def tearDown(self): self.owned.tearDown()

    def test_identity_probe_failure_still_reaps_and_restores_handlers(self):
        real_popen, children = subprocess.Popen, []
        def capture(command, **kwargs):
            process = real_popen(command, **kwargs)
            if kwargs.get("start_new_session"): children.append(process)
            return process
        calls = 0
        def failed(path):
            nonlocal calls
            if path == self.owned.output / "live":
                calls += 1
                if calls == 3: raise OSError("owned synthetic ownership probe failure")
            return directory_identity(path)
        handlers = {sig: signal.getsignal(sig) for sig in (signal.SIGTERM, signal.SIGINT)}
        def extra(stack):
            stack.enter_context(patch("t22_pair_runtime.subprocess.Popen", side_effect=capture))
            stack.enter_context(patch("t22_pair_runtime.directory_identity", side_effect=failed))
        self.assertEqual(self.owned.run_preparer("encrypted", extra), 1)
        self.assertEqual(len(children), 1); self.assertIsNotNone(children[0].returncode)
        self.assertFalse(group_alive(children[0].pid))
        self.assertFalse(self.owned.receipt()["cleanup_complete"])
        self.assertTrue(self.owned.receipt()["cleanup_required"])
        self.assertFalse((self.owned.output / "t22-input-manifest.tsv").exists())
        self.assertEqual({sig: signal.getsignal(sig) for sig in handlers}, handlers)

    def test_interrupted_wait_reaps_known_child_without_claiming_shutdown(self):
        real_popen, children = subprocess.Popen, []
        def capture(command, **kwargs):
            process = real_popen(command, **kwargs)
            if kwargs.get("start_new_session"):
                children.append(process); wait, first = process.wait, True
                def interrupted(*args, **kwargs):
                    nonlocal first
                    if first:
                        first = False; raise KeyboardInterrupt()
                    return wait(*args, **kwargs)
                process.wait = interrupted
            return process
        def extra(stack): stack.enter_context(patch("t22_pair_runtime.subprocess.Popen", side_effect=capture))
        self.assertEqual(self.owned.run_preparer(more=extra), 1)
        self.assertEqual(len(children), 1); self.assertIsNotNone(children[0].returncode)
        self.assertFalse(group_alive(children[0].pid))
        self.assertTrue(self.owned.receipt()["cleanup_complete"])
        self.assertFalse(self.owned.receipt()["natural_shutdown_observed"])
        self.assertFalse((self.owned.output / "t22-input-manifest.tsv").exists())


if __name__ == "__main__": unittest.main()
