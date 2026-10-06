#!/usr/bin/env python3
"""Actual bounded adapter children and unknown constructor ownership fail closed."""
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
from t22_pair_queue_proof import adapter_context, gone


class AdapterProcess(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="bridgevm-d10-child-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.identity = {"commit": "a" * 40, "job_id": "owned-fixture"}
        self.handlers = {sig: signal.getsignal(sig) for sig in (signal.SIGTERM, signal.SIGINT)}

    def tearDown(self):
        self.assertEqual({sig: signal.getsignal(sig) for sig in self.handlers}, self.handlers)
        for path in self.root.rglob("*"): path.chmod(0o700 if path.is_dir() else 0o600)

    def command(self, body): return [sys.executable, "-c", body, str(self.root)]

    def test_actual_completed_child_has_pinned_command_and_observed_terminal(self):
        argv = self.command("raise SystemExit(0)")
        self.assertEqual(launch(argv, self.root, self.identity, timeout=2), 0)
        value, digest = adapter_context(self.root, self.identity, command_hash(argv))
        self.assertEqual(value["cause"], "completed")
        self.assertEqual(value["exit_code"], 0)
        self.assertEqual((self.root / "adapter-launch-context.json").stat().st_mode & 0o777, 0o400)
        with self.assertRaises(ValueError): adapter_context(self.root, self.identity, "0" * 64)

    def test_actual_timeout_reaps_core_but_claims_no_native_cleanup(self):
        argv = self.command("import time; time.sleep(30)")
        self.assertEqual(launch(argv, self.root, self.identity, timeout=.05), 1)
        value, _ = adapter_context(self.root, self.identity, command_hash(argv))
        self.assertEqual(value["cause"], "timeout")
        self.assertTrue(value["terminal_observed"])
        self.assertNotIn("worker_cleanup_verified", value)
        with self.assertRaises(ProcessLookupError): os.kill(value["core_pid"], 0)

    def test_constructor_interrupt_preserves_attempt_without_invented_pid(self):
        for error in (KeyboardInterrupt, InterruptedError):
            directory = self.root / error.__name__; directory.mkdir()
            argv = self.command("raise SystemExit(0)")
            with patch("t22_pair_queue_command.subprocess.Popen", side_effect=error):
                self.assertEqual(launch(argv, directory, self.identity), 1)
            self.assertTrue((directory / "adapter-launch-attempt.json").is_file())
            self.assertFalse((directory / "adapter-launch-context.json").exists())
            with self.assertRaises(OSError): adapter_context(directory, self.identity, command_hash(argv))

    def test_interrupted_wait_reaps_known_child_and_records_refusal(self):
        children, real = [], subprocess.Popen
        def spawn(*args, **kwargs):
            child = real(*args, **kwargs); children.append(child)
            wait, first = child.wait, True
            def interrupted(*arguments, **options):
                nonlocal first
                if first: first = False; raise KeyboardInterrupt()
                return wait(*arguments, **options)
            child.wait = interrupted
            return child
        argv = self.command("import time; time.sleep(30)")
        with patch("t22_pair_queue_command.subprocess.Popen", side_effect=spawn):
            self.assertEqual(launch(argv, self.root, self.identity), 1)
        value, _ = adapter_context(self.root, self.identity, command_hash(argv))
        self.assertEqual(value["cause"], "interrupted")
        self.assertIsNotNone(children[0].returncode)
        self.assertNotIn("group_absence_observed", value)

    def test_mocked_unqueryable_group_refuses_without_a_nonzero_signal(self):
        child = subprocess.Popen(self.command("import time; time.sleep(30)"), start_new_session=True)
        try:
            with patch("t22_pair_queue_proof.os.killpg", side_effect=PermissionError) as probe:
                with self.assertRaises(PermissionError): gone(child.pid, group=True)
                probe.assert_called_once_with(child.pid, 0)
            self.assertIsNone(child.poll())
            for pid in (None, True, 0, 1, -1, 2 ** 31):
                with self.subTest(pid=pid):
                    with self.assertRaises(ValueError): gone(pid, group=True)
        finally:
            child.terminate(); child.wait(timeout=5)


if __name__ == "__main__": unittest.main()
