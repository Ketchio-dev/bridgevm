"""Failure diagnostics preserve exact observer errors and owned child status."""
import json
from pathlib import Path
import signal
import subprocess
import tempfile
from unittest.mock import Mock, patch

import a19_interrupt_restore_child as observer
import a19_interrupt_failure as diagnostics
from a19_interrupt_child_fixture import InterruptedChildCases as ExistingChildCases
from a19_interrupt_cli_cases import CLIArgumentCases

FAILURE = "interrupt-helper-failure.private.json"


class InterruptedChildCases(ExistingChildCases, CLIArgumentCases):
    def failure_receipt(self, output, code):
        path = output / FAILURE
        self.assertTrue(path.is_file(), "owned child failure status was not retained")
        value = json.loads(path.read_text())
        self.assertEqual(value["schema_version"], 1)
        self.assertFalse(value["pass"])
        self.assertEqual(value["interruption_stage"], "staged-disk-verify-read")
        self.assertEqual(value["helper_pid"], 4242)
        self.assertEqual(value["helper_returncode"], code)
        self.assertEqual(value["helper_returncode_known"], code is not None)
        self.assertEqual(value["status_source"], "Popen.returncode")
        self.assertEqual(set(value), {"schema_version", "pass", "interruption_stage", "helper_pid",
                                     "helper_returncode", "helper_returncode_known", "status_source", "failure_type"})
        self.assertEqual(path.stat().st_mode & 0o777, 0o600)
        self.assert_no_observation(output)
        return value

    def test_failure_receipt_keeps_early_exit_zero_nonzero_and_signal(self):
        for code in (0, 23, -signal.SIGTERM):
            with self.subTest(code=code), tempfile.TemporaryDirectory() as temporary:
                helper, snapshot, disk, variables, output = self.make_child(Path(temporary))
                child = Mock(pid=4242, returncode=code, _a19_reaped=False)
                child.poll.return_value = child.wait.return_value = code
                with patch.object(observer.subprocess, "Popen", return_value=child):
                    with self.assertRaisesRegex(RuntimeError, "^snapshot helper exited before staged read$"):
                        observer.run(helper, snapshot, disk, variables, output, 1)
                value = self.failure_receipt(output, code)
                self.assertEqual(value["failure_type"], "RuntimeError")
                child.kill.assert_not_called()
                child.wait.assert_called_once_with(timeout=5)
                self.assertEqual(disk.read_bytes(), b"old-disk")
                self.assertEqual(variables.read_bytes(), b"old-vars")

    def test_failure_receipt_records_timeout_cleanup_status_without_acceptance(self):
        with tempfile.TemporaryDirectory() as temporary:
            helper, snapshot, disk, variables, output = self.make_child(Path(temporary))
            child = Mock(pid=4242, returncode=None, _a19_reaped=False)
            child.poll.return_value = None
            child.kill.side_effect = lambda: setattr(child, "returncode", -signal.SIGKILL)
            child.wait.return_value = -signal.SIGKILL
            error = TimeoutError("no authenticated staged read before deadline")
            with patch.object(observer.subprocess, "Popen", return_value=child), \
                    patch.object(observer, "stop_at_verified_stage", side_effect=error):
                with self.assertRaises(TimeoutError) as raised:
                    observer.run(helper, snapshot, disk, variables, output, 1)
            self.assertIs(raised.exception, error)
            self.failure_receipt(output, -signal.SIGKILL)
            child.kill.assert_called_once_with()
            child.wait.assert_called_once_with(timeout=5)

    def test_failure_receipt_does_not_invent_exit_when_reap_is_unconfirmed(self):
        with tempfile.TemporaryDirectory() as temporary:
            helper, snapshot, disk, variables, output = self.make_child(Path(temporary))
            child = Mock(pid=4242, returncode=None, _a19_reaped=False)
            child.poll.return_value = None
            failure = subprocess.TimeoutExpired(["owned-helper"], 5)
            child.wait.side_effect = failure
            with patch.object(observer.subprocess, "Popen", return_value=child), \
                    patch.object(observer, "stop_at_verified_stage", side_effect=TimeoutError("stage deadline")):
                with self.assertRaises(subprocess.TimeoutExpired) as raised:
                    observer.run(helper, snapshot, disk, variables, output, 1)
            self.assertIs(raised.exception, failure)
            value = self.failure_receipt(output, None)
            self.assertEqual(value["failure_type"], "TimeoutExpired")

    def test_failure_receipt_collision_preserves_existing_record_and_original_error(self):
        with tempfile.TemporaryDirectory() as temporary:
            helper, snapshot, disk, variables, output = self.make_child(Path(temporary))
            record = output / FAILURE
            record.write_bytes(b"existing diagnostic")
            child = Mock(pid=4242, returncode=19, _a19_reaped=False)
            child.poll.return_value = child.wait.return_value = 19
            with patch.object(observer.subprocess, "Popen", return_value=child):
                with self.assertRaisesRegex(RuntimeError, "^snapshot helper exited before staged read$") as raised:
                    observer.run(helper, snapshot, disk, variables, output, 1)
            self.assertEqual(record.read_bytes(), b"existing diagnostic")
            if hasattr(BaseException, "add_note"):
                self.assertIn("failure receipt could not be retained", getattr(raised.exception, "__notes__", ()))
            self.assert_no_observation(output)

    def test_failure_receipt_not_created_when_no_child_was_launched(self):
        with tempfile.TemporaryDirectory() as temporary:
            helper, snapshot, disk, variables, output = self.make_child(Path(temporary))
            failure = OSError("spawn refused")
            with patch.object(observer.subprocess, "Popen", side_effect=failure):
                with self.assertRaises(OSError) as raised:
                    observer.run(helper, snapshot, disk, variables, output, 1)
            self.assertIs(raised.exception, failure)
            self.assertFalse((output / FAILURE).exists())
            self.assert_no_observation(output)

    def test_failure_receipt_symlink_preserves_target_and_original_exception(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary)
            target = output / "protected"
            target.write_bytes(b"keep diagnostic target")
            (output / FAILURE).symlink_to(target)
            error = RuntimeError("original failure")
            with self.assertRaises(RuntimeError) as raised:
                with diagnostics.FailedObservation(output, "staged-disk-verify-read") as failure:
                    failure.child = Mock(pid=4242, returncode=0)
                    raise error
            self.assertIs(raised.exception, error)
            self.assertEqual(target.read_bytes(), b"keep diagnostic target")
            self.assertTrue((output / FAILURE).is_symlink())

    def test_failure_receipt_write_refusal_without_exception_notes_preserves_error(self):
        error = RuntimeError("original failure")
        with patch.object(diagnostics, "private_new", side_effect=PermissionError("receipt refused")), \
                patch.object(diagnostics, "BaseException", object, create=True):
            with self.assertRaises(RuntimeError) as raised:
                with diagnostics.FailedObservation(Path("unused"), "staged-disk-verify-read") as failure:
                    failure.child = Mock(pid=4242, returncode=0)
                    raise error
        self.assertIs(raised.exception, error)

    def test_failure_receipt_absent_for_accepted_mock_observation(self):
        with tempfile.TemporaryDirectory() as temporary:
            helper, snapshot, disk, variables, output = self.make_child(Path(temporary))
            child = Mock(pid=4242, returncode=-signal.SIGKILL, _a19_reaped=True)
            stopped = {"interruption_stage": "staged-disk-verify-read"}
            with patch.object(observer.subprocess, "Popen", return_value=child), \
                    patch.object(observer, "stop_at_verified_stage", return_value=stopped):
                result = observer.run(helper, snapshot, disk, variables, output, 1)
            self.assertTrue(result["helper_killed_and_reaped"])
            self.assertFalse((output / FAILURE).exists())
            self.assertTrue((output / "interrupt-helper-context.private.json").is_file())
            child.poll.assert_not_called()
            child.kill.assert_not_called()
            child.wait.assert_not_called()

    def test_failure_receipt_records_actual_exited_fixture_status(self):
        with tempfile.TemporaryDirectory() as temporary:
            helper, snapshot, disk, variables, output = self.make_child(Path(temporary))
            helper.write_text("#!/usr/bin/env python3\nraise SystemExit(23)\n")
            spawn = subprocess.Popen
            children = []

            def exited_child(*args, **kwargs):
                child = spawn(*args, **kwargs)
                children.append(child)
                child.wait(timeout=5)
                return child

            try:
                with patch.object(observer.subprocess, "Popen", side_effect=exited_child):
                    with self.assertRaisesRegex(RuntimeError, "^snapshot helper exited before staged read$"):
                        observer.run(helper, snapshot, disk, variables, output, 1)
                self.assertTrue((output / FAILURE).is_file(), "actual child failure status was not retained")
                value = json.loads((output / FAILURE).read_text())
                self.assertEqual(value["helper_pid"], children[0].pid)
                self.assertEqual(value["helper_returncode"], 23)
                self.assertTrue(value["helper_returncode_known"])
                self.assert_no_observation(output)
            finally:
                for child in children:
                    if child.poll() is None:
                        child.kill()
                    child.wait(timeout=5)
