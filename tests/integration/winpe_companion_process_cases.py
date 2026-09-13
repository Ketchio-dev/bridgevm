"""Owned-process lifecycle and actual D4 no-after-work sequencing contracts."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/live-gates"))
import winpe_companion_process as PROCESS
from guest_input_group_liveness import group_alive


def runner_module():
    spec = importlib.util.spec_from_file_location(
        "winpe_runner_process_safety", ROOT / "scripts/live-gates/run-winpe-companions.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def job_fields():
    return {"tier": "d4-winpe-companions", "job_id": "process-safety-fixture",
            "commit": "a" * 40, "input_manifest_sha256": "b" * 64,
            "sealed_binary_sha256": "c" * 64}


class WinPEProcessSafetyTests(unittest.TestCase):
    def test_normal_and_nonzero_exit_both_stop_the_owned_group(self):
        for returncode in (0, 7, -15):
            with self.subTest(returncode=returncode):
                process = mock.Mock()
                process.wait.return_value = returncode
                with mock.patch.object(PROCESS, "Popen", return_value=process) as spawn, \
                     mock.patch.object(PROCESS, "stop", return_value=True) as stop:
                    result = PROCESS.run_owned(["fixture"], timeout=1)
                self.assertEqual(result.returncode, returncode)
                self.assertEqual(result.args, ["fixture"])
                self.assertIs(spawn.call_args.kwargs["start_new_session"], True)
                stop.assert_called_once_with(process)

    def test_timeout_stops_before_raising(self):
        process = mock.Mock()
        process.wait.side_effect = subprocess.TimeoutExpired(["fixture"], 0.1)
        with mock.patch.object(PROCESS, "Popen", return_value=process), \
             mock.patch.object(PROCESS, "monotonic", side_effect=[10, 10, 11]), \
             mock.patch.object(PROCESS, "stop", return_value=True) as stop:
            with self.assertRaises(PROCESS.OwnedProcessError) as raised:
                PROCESS.run_owned(["fixture"], timeout=0.5)
        self.assertIs(raised.exception.cleanup_complete, True)
        self.assertIsInstance(raised.exception.__cause__, subprocess.TimeoutExpired)
        stop.assert_called_once_with(process)

    def test_wait_exceptions_still_stop_and_preserve_failure(self):
        for cause in (OSError("wait failed"), KeyboardInterrupt(), SystemExit(3)):
            with self.subTest(cause=type(cause).__name__):
                process = mock.Mock()
                process.wait.side_effect = cause
                with mock.patch.object(PROCESS, "Popen", return_value=process), \
                     mock.patch.object(PROCESS, "stop", return_value=True) as stop:
                    with self.assertRaises(PROCESS.OwnedProcessError) as raised:
                        PROCESS.run_owned(["fixture"], timeout=1)
                self.assertIs(raised.exception.cleanup_complete, True)
                self.assertIs(raised.exception.__cause__, cause)
                stop.assert_called_once_with(process)

    def test_spawn_error_without_handle_does_not_claim_cleanup(self):
        with mock.patch.object(PROCESS, "Popen", side_effect=OSError("spawn failed")), \
             mock.patch.object(PROCESS, "stop") as stop:
            with self.assertRaises(PROCESS.OwnedProcessError) as raised:
                PROCESS.run_owned(["fixture"], timeout=1)
        self.assertIs(raised.exception.cleanup_complete, False)
        stop.assert_not_called()

    def test_stop_requires_literal_true(self):
        for answer in (False, None, 1, "true"):
            with self.subTest(answer=answer):
                process = mock.Mock()
                process.wait.return_value = 0
                with mock.patch.object(PROCESS, "Popen", return_value=process), \
                     mock.patch.object(PROCESS, "stop", return_value=answer):
                    with self.assertRaises(PROCESS.OwnedProcessError) as raised:
                        PROCESS.run_owned(["fixture"], timeout=1)
                self.assertIs(raised.exception.cleanup_complete, False)

    def test_stop_exceptions_withhold_cleanup(self):
        for cause in (OSError("stop failed"), KeyboardInterrupt()):
            with self.subTest(cause=type(cause).__name__):
                process = mock.Mock()
                process.wait.return_value = 0
                with mock.patch.object(PROCESS, "Popen", return_value=process), \
                     mock.patch.object(PROCESS, "stop", side_effect=cause):
                    with self.assertRaises(PROCESS.OwnedProcessError) as raised:
                        PROCESS.run_owned(["fixture"], timeout=1)
                self.assertIs(raised.exception.cleanup_complete, False)
                self.assertIs(raised.exception.__cause__, cause)

    def test_sigterm_and_sigint_cancel_even_when_wait_returns_zero(self):
        for number in (signal.SIGTERM, signal.SIGINT):
            with self.subTest(number=number):
                previous = {item: signal.getsignal(item) for item in (signal.SIGTERM, signal.SIGINT)}
                process = mock.Mock()
                def wait(**_kwargs):
                    os.kill(os.getpid(), number)
                    return 0
                process.wait.side_effect = wait
                with mock.patch.object(PROCESS, "Popen", return_value=process), \
                     mock.patch.object(PROCESS, "stop", return_value=True) as stop:
                    with self.assertRaises(PROCESS.OwnedProcessError) as raised:
                        PROCESS.run_owned(["fixture"], timeout=1)
                self.assertIs(raised.exception.cleanup_complete, True)
                stop.assert_called_once_with(process)
                for item, handler in previous.items():
                    self.assertEqual(signal.getsignal(item), handler)

    def test_repeated_signals_cannot_interrupt_cleanup(self):
        process = mock.Mock()
        process.wait.return_value = 0
        reached = []
        def stop(_process):
            for number in (signal.SIGTERM, signal.SIGINT, signal.SIGTERM):
                os.kill(os.getpid(), number)
                reached.append(number)
            return True
        with mock.patch.object(PROCESS, "Popen", return_value=process), \
             mock.patch.object(PROCESS, "stop", side_effect=stop):
            with self.assertRaises(PROCESS.OwnedProcessError) as raised:
                PROCESS.run_owned(["fixture"], timeout=1)
        self.assertEqual(len(reached), 3)
        self.assertIs(raised.exception.cleanup_complete, True)

    def test_partial_signal_install_restores_previous_handlers(self):
        previous = {number: signal.getsignal(number) for number in (signal.SIGTERM, signal.SIGINT)}
        original = signal.signal
        calls = []
        def install(number, handler):
            calls.append(number)
            if len(calls) == 2:
                raise ValueError("fixture signal install failed")
            return original(number, handler)
        with mock.patch.object(PROCESS.signal, "signal", side_effect=install), \
             mock.patch.object(PROCESS, "Popen") as spawn, \
             mock.patch.object(PROCESS, "stop") as stop:
            with self.assertRaises(PROCESS.OwnedProcessError) as raised:
                PROCESS.run_owned(["fixture"], timeout=1)
        self.assertIs(raised.exception.cleanup_complete, True)
        spawn.assert_not_called()
        stop.assert_not_called()
        for number, handler in previous.items():
            self.assertEqual(signal.getsignal(number), handler)

    def test_invalid_timeouts_do_not_spawn_or_signal_a_group(self):
        for timeout in (0, -1, float("nan"), float("inf"), True):
            with self.subTest(timeout=timeout), mock.patch.object(PROCESS, "Popen") as spawn, \
                 mock.patch.object(PROCESS, "stop") as stop:
                with self.assertRaises(PROCESS.OwnedProcessError) as raised:
                    PROCESS.run_owned(["fixture"], timeout=timeout)
                self.assertIs(raised.exception.cleanup_complete, True)
                spawn.assert_not_called()
                stop.assert_not_called()

    def test_child_ignoring_term_is_removed_after_leader_exit(self):
        original_spawn, original_stop = PROCESS.Popen, PROCESS.stop
        spawned = []
        def spawn(*args, **kwargs):
            process = original_spawn(*args, **kwargs)
            spawned.append(process)
            return process
        child = ("import os,pathlib,signal,sys,time; "
                 "signal.signal(signal.SIGTERM,signal.SIG_IGN); "
                 "pathlib.Path(sys.argv[1]).write_text(str(os.getpid())); time.sleep(30)")
        leader = ("import pathlib,subprocess,sys,time\n"
                  "subprocess.Popen([sys.executable,'-c',sys.argv[1],sys.argv[2]])\n"
                  "deadline=time.monotonic()+5\n"
                  "while not pathlib.Path(sys.argv[2]).exists() and time.monotonic()<deadline: time.sleep(0.01)\n"
                  "sys.exit(0 if pathlib.Path(sys.argv[2]).exists() else 2)\n")
        with tempfile.TemporaryDirectory(prefix="bridgevm-owned-process-") as directory:
            marker = Path(directory) / "child-ready"
            try:
                with mock.patch.object(PROCESS, "Popen", side_effect=spawn), \
                     mock.patch.object(PROCESS, "stop", side_effect=lambda p: original_stop(p, grace=0.1)):
                    result = PROCESS.run_owned([sys.executable, "-c", leader, child, str(marker)],
                                               timeout=10, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                self.assertEqual(result.returncode, 0)
                self.assertTrue(marker.is_file())
                self.assertGreater(int(marker.read_text()), 1)
                self.assertEqual(len(spawned), 1)
                self.assertFalse(group_alive(spawned[0].pid))
            finally:
                for process in spawned:
                    self.assertIs(original_stop(process, grace=0.1), True)

    def test_actual_runner_skips_after_work_after_process_failure(self):
        runner = runner_module()
        for safe in (False, True):
            with self.subTest(cleanup_complete=safe), tempfile.TemporaryDirectory() as directory:
                out = Path(directory)
                manifest, binary = out / "input.json", out / "probe"
                manifest.write_text("{}\n")
                binary.write_bytes(b"fixture")
                records = {name: (out / name, "c" * 64 if name == "binary" else "d" * 64)
                           for name in ("image", "vars", "injector", "binary", "firmware")}
                clones = {name: mock.Mock(spec=Path, name="clone-" + name)
                          for name in ("image", "vars", "injector")}
                inspect = mock.Mock(return_value={"available": True, "files": {}})
                verify, compare = mock.Mock(), mock.Mock()
                hashes = mock.Mock(side_effect=lambda path: "b" * 64 if path == manifest else "d" * 64)
                execute = mock.Mock(side_effect=PROCESS.OwnedProcessError("fixture", cleanup_complete=safe))
                with mock.patch.multiple(runner, job_fields=mock.Mock(return_value=job_fields()),
                                         clone=mock.Mock(return_value=clones), load=mock.Mock(return_value=records),
                                         inspect=inspect, verify=verify, compare=compare, file_hash=hashes,
                                         run_owned=execute), \
                     mock.patch.object(runner.subprocess, "check_output", return_value="a" * 40), \
                     mock.patch.object(runner.platform, "system", return_value="Darwin"), \
                     mock.patch.object(runner.platform, "machine", return_value="arm64"):
                    args = argparse.Namespace(out=out, job_id=job_fields()["job_id"],
                                              input_manifest=manifest, sealed_binary=binary)
                    self.assertEqual(runner.run(args), 1)
                execute.assert_called_once()
                inspect.assert_called_once()
                verify.assert_not_called()
                compare.assert_not_called()
                self.assertTrue(all(call.args[0] not in clones.values() for call in hashes.call_args_list))
                for path in clones.values():
                    path.chmod.assert_not_called()
                receipt = json.loads((out / "receipt.json").read_text())
                self.assertEqual(receipt["failure_stage"], "execute")
                self.assertIs(receipt["cleanup_complete"], safe)
                self.assertIs(receipt["mount_cleanup_complete"], False)
                self.assertEqual(receipt["outcome"], "diagnostic-incomplete")

    def test_complete_receipt_requires_each_safety_proof(self):
        runner = runner_module()
        job = job_fields()
        complete = runner.initial(job)
        complete.update(sample_count=1, source_integrity_verified=True, execution_exit_code=0,
                        outcome="diagnostic-complete", cleanup_complete=True, mount_cleanup_complete=True)
        runner.validate(complete, job)
        for key in ("cleanup_complete", "mount_cleanup_complete"):
            for missing in (False, True):
                with self.subTest(key=key, missing=missing):
                    invalid = dict(complete)
                    if missing:
                        del invalid[key]
                    else:
                        invalid[key] = False
                    with self.assertRaises(ValueError):
                        runner.validate(invalid, job)


if __name__ == "__main__":
    unittest.main()
