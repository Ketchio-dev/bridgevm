#!/usr/bin/env python3
"""Actual job-control timeout retention and failed production receipt contracts."""
from __future__ import annotations

import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import MagicMock, patch

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/live-gates"))
import a19_lifecycle_process as lifecycle
import a19_interrupted_restore_receipt as receipt
from a19_collection_fixtures import added_cases, first_case
from a19_process_cleanup_fixture import ProcessRunnerFixture
from a19_prepared_inputs_cleanup_cases import PreparedInputsCleanupCases


def fixture_repo(root: Path) -> Path:
    repo = root / "fixture-repo"
    script = repo / "scripts/verify-native-snapshot-interrupted-restore.sh"
    script.parent.mkdir(parents=True)
    script.write_text('#!/bin/bash\nset -m\n'
        'echo $$ > "$OUT/shell-pid"\n'
        'sleep 30 &\necho $! > "$OUT/background-pid"\n'
        '/bin/bash -c \'echo $$ > "$OUT/foreground-pid"; sleep 30\'\n')
    script.chmod(0o700)
    return repo


def cleanup_fixture_groups(output: Path) -> None:
    for name in ("background-pid", "foreground-pid", "shell-pid"):
        path = output / name
        if not path.exists():
            continue
        pid = int(path.read_text())
        try:
            if os.getpgid(pid) != pid:
                raise AssertionError("fixture did not own the recorded process group")
            os.killpg(pid, signal.SIGKILL)
        except ProcessLookupError:
            pass


class ProcessCleanup(PreparedInputsCleanupCases, ProcessRunnerFixture, unittest.TestCase):
    def test_actual_hung_foreground_and_job_control_child_leave_fenced_failed_receipt(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            output = root / "cleanup-fixture"
            output.mkdir()
            repo = fixture_repo(root)
            try:
                def operation(_repo, environment):
                    return lifecycle.run_lifecycle(repo, environment, 1.5, 0.2, 0.2)
                self.assertEqual(self.run_main(output, operation), 1)
                self.assertTrue((output / "prepared-inputs/disk.raw").is_file(), self.last_error)
                self.assertFalse((output / "live").exists())
                for name in ("background-pid", "foreground-pid"):
                    pid = int((output / name).read_text())
                    self.assertEqual(os.getpgid(pid), pid)
                    os.kill(pid, 0)
                value = receipt.validate(json.loads((output / "receipt.json").read_text()))
                self.assertEqual(value["schema_version"], 2)
                self.assertFalse(value["pass"] or value["worker_cleanup_verified"])
                self.assertEqual([value[key] for key in ("sample_count", "run_count", "interruption_case_count")], [0, 0, 0])
                self.assertTrue(all(value[key] is False for key in ("swap_retry_succeeded", "create_retry_succeeded")))
            finally:
                cleanup_fixture_groups(output)

    def test_unreaped_child_and_process_probe_error_are_cleanup_uncertain(self):
        for probe_error in (False, True):
            with self.subTest(probe_error=probe_error):
                child = MagicMock(pid=123)
                child.poll.return_value = None
                child.wait.side_effect = (None if probe_error else [
                    subprocess.TimeoutExpired("fixture", 1)] * 3)
                child.wait.return_value = 0
                with patch.object(lifecycle.subprocess, "Popen", return_value=child), \
                        patch.object(lifecycle.os, "killpg", side_effect=OSError("fixture probe") if probe_error else None):
                    with self.assertRaises(lifecycle.LifecycleCleanupUncertain):
                        lifecycle.run_lifecycle(Path("/fixture"), {}, 0.01, 0.01, 0.01)

    def test_spawn_or_wait_interrupt_keeps_prepared_media_without_a_live_directory(self):
        for constructor, interruption in ((True, InterruptedError), (True, KeyboardInterrupt),
                                          (False, InterruptedError), (False, KeyboardInterrupt)):
            with self.subTest(constructor=constructor, interruption=interruption), \
                    tempfile.TemporaryDirectory() as temporary:
                output = Path(temporary) / "cleanup-fixture"
                output.mkdir()
                child = MagicMock(pid=123)
                child.poll.return_value = None
                child.wait.side_effect = [interruption("fixture interruption"), 0]
                signals = MagicMock()
                def operation(repo, environment):
                    with patch.object(lifecycle.subprocess, "Popen",
                                      side_effect=interruption("fixture spawn") if constructor else None,
                                      return_value=child), patch.object(lifecycle.os, "killpg", signals):
                        return lifecycle.run_lifecycle(repo, environment, 0.01, 0.01, 0.01)
                try:
                    self.assertEqual(self.run_main(output, operation), 1)
                except KeyboardInterrupt:
                    self.fail("interrupt escaped lifecycle ownership fencing")
                value = receipt.validate(json.loads((output / "receipt.json").read_text()))
                self.assertTrue((output / "prepared-inputs/disk.raw").is_file(), self.last_error)
                self.assertFalse((output / "live").exists())
                self.assertFalse(value["pass"] or value["worker_cleanup_verified"])
                self.assertEqual([value[key] for key in ("sample_count", "run_count", "interruption_case_count")], [0, 0, 0])
                if constructor:
                    signals.assert_not_called()

    def test_failed_passing_receipt_prerequisites_still_write_honest_failed_schema_two(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "cleanup-fixture"
            output.mkdir()
            def operation(_repo, _environment):
                added_cases(output, first_case(output))
                return 0
            self.assertEqual(self.run_main(output, operation, complete=True), 1)
            value = receipt.validate(json.loads((output / "receipt.json").read_text()))
            self.assertFalse(value["pass"])
            self.assertTrue(value["worker_cleanup_verified"])
            self.assertEqual(value["outcome"], "failed", self.last_error)
            self.assertIn("passing T22 receipt changed prepared_image_sha256", self.last_error)
            self.assertEqual([value[key] for key in ("sample_count", "run_count", "interruption_case_count")], [0, 0, 0])
            self.assertFalse((output / "prepared-inputs").exists())


if __name__ == "__main__":
    unittest.main()
