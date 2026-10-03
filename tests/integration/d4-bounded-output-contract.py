#!/usr/bin/env python3
"""Finite byte-stream fixtures for D4's evidence quota and refusal boundaries."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
HELPER = ROOT / "scripts/live-gates/bounded_output.py"
sys.path.insert(0, str(HELPER.parent))
import bounded_output as OUTPUT
from winpe_companion_process import OwnedProcessError


class BoundedOutputContract(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="bridgevm-bounded-output-")
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name)
        self.output = self.directory / "output.log"
        self.status = self.directory / "status.json"

    def context_stream(self, data, limit):
        with OUTPUT.BoundedOutput(self.output, self.status, limit) as sink:
            result = subprocess.run(
                [sys.executable, "-c", "import os,sys; os.write(1,bytes.fromhex(sys.argv[1]))", data.hex()],
                stdout=sink, stderr=subprocess.STDOUT, timeout=3)
        self.assertEqual(result.returncode, 0)
        return sink

    def test_empty_and_exact_limit_preserve_binary_bytes(self):
        for data in (b"", b"\x00\xff\xfe\n" * 16):
            with self.subTest(length=len(data)):
                sink = self.context_stream(data, max(1, len(data)))
                record = sink.check()
                self.assertEqual(self.output.read_bytes(), data)
                self.assertEqual(record["observed_bytes"], len(data))
                self.assertEqual(record["stored_bytes"], len(data))
                self.assertTrue(record["complete"])
                self.output.unlink(); self.status.unlink()

    def test_cap_plus_one_is_explicitly_refused(self):
        sink = self.context_stream(b"12345", 4)
        with self.assertRaises(ValueError):
            sink.check()
        self.assertEqual(self.output.read_bytes(), b"1234")
        record = OUTPUT.read_status(self.status, 4, require_complete=False, output=self.output)
        self.assertEqual((record["observed_bytes"], record["stored_bytes"]), (5, 4))
        self.assertTrue(record["overflow"])
        self.assertFalse(record["complete"])
        self.assertEqual(record["error"], "overflow")

    def test_long_binary_stream_drains_after_overflow_to_natural_exit(self):
        size = 256 * 1024
        with OUTPUT.BoundedOutput(self.output, self.status, 1024) as sink:
            result = subprocess.run([sys.executable, "-c",
                "import os; data=b'\\xff'*4096; [os.write(1,data) for _ in range(64)]"],
                stdout=sink, stderr=subprocess.STDOUT, timeout=3)
        self.assertEqual(result.returncode, 0)
        with self.assertRaises(ValueError): sink.check()
        record = OUTPUT.read_status(self.status, 1024, require_complete=False)
        self.assertEqual(record["observed_bytes"], size)
        self.assertEqual(self.output.read_bytes(), b"\xff" * 1024)

    def test_under_limit_long_line_is_complete(self):
        data = b"\xff\x00" * 40000
        with OUTPUT.BoundedOutput(self.output, self.status, len(data)) as sink:
            result = subprocess.run([sys.executable, "-c",
                "import os; [os.write(1,b'\\xff\\x00'*10000) for _ in range(4)]"],
                stdout=sink, stderr=subprocess.STDOUT, timeout=3)
        self.assertEqual(result.returncode, 0)
        sink.check()
        self.assertEqual(self.output.read_bytes(), data)

    def test_initial_status_readiness_and_cli_completion(self):
        process = subprocess.Popen([sys.executable, str(HELPER), "--output", str(self.output),
            "--status", str(self.status), "--limit", "4", "--token", "fixture-reader"], stdin=subprocess.PIPE,
            stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        try:
            deadline = time.monotonic() + 2
            while not self.status.exists() and time.monotonic() < deadline:
                time.sleep(0.01)
            ready = subprocess.run([sys.executable, str(HELPER), "--check-ready", str(self.status),
                "--limit", "4", "--token", "fixture-reader"], capture_output=True, timeout=3)
            self.assertEqual(ready.returncode, 0, ready.stderr)
            record = OUTPUT.read_status(self.status, 4, require_complete=False, output=self.output)
            self.assertEqual((record["observed_bytes"], record["stored_bytes"]), (0, 0))
            self.assertFalse(record["complete"])
            process.communicate(b"\xffabc", timeout=3)
            self.assertEqual(process.returncode, 0)
            OUTPUT.read_status(self.status, 4, output=self.output)
            self.assertEqual(self.output.read_bytes(), b"\xffabc")
            ready = subprocess.run([sys.executable, str(HELPER), "--check-ready", str(self.status),
                "--limit", "4", "--token", "fixture-reader"], capture_output=True, timeout=3)
            self.assertNotEqual(ready.returncode, 0)
        finally:
            if process.poll() is None:
                process.kill(); process.communicate(timeout=3)

    def test_cli_overflow_drains_and_exits_nonzero(self):
        result = subprocess.run([sys.executable, str(HELPER), "--output", str(self.output),
            "--status", str(self.status), "--limit", "3", "--token", "fixture-reader"], input=b"\xff" * 100000,
            capture_output=True, timeout=3)
        self.assertNotEqual(result.returncode, 0)
        record = OUTPUT.read_status(self.status, 3, require_complete=False, output=self.output)
        self.assertEqual(record["observed_bytes"], 100000)
        self.assertEqual(self.output.read_bytes(), b"\xff" * 3)
        self.assertEqual(result.stderr, b"bounded output refused\n")

    def test_cli_same_size_output_replacement_is_durable_refusal(self):
        process = subprocess.Popen([sys.executable, str(HELPER), "--output", str(self.output),
            "--status", str(self.status), "--limit", "4", "--token", "replacement-fixture"],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        try:
            process.stdin.write(b"good"); process.stdin.flush()
            deadline = time.monotonic() + 2
            while (not self.output.exists() or self.output.stat().st_size != 4) and time.monotonic() < deadline:
                time.sleep(0.01)
            self.assertEqual(self.output.stat().st_size, 4)
            self.output.unlink(); self.output.write_bytes(b"evil")
            process.stdin.close(); process.stdin = None
            process.communicate(timeout=3)
            self.assertNotEqual(process.returncode, 0)
            record = OUTPUT.read_status(self.status, 4, require_complete=False)
            self.assertEqual(record["error"], "output-io")
            self.assertFalse(record["complete"])
        finally:
            if process.poll() is None:
                process.kill(); process.communicate(timeout=3)

    def test_context_preserves_cleanup_uncertainty_exception(self):
        failure = OwnedProcessError("fixture", cleanup_complete=False)
        with self.assertRaises(OwnedProcessError) as caught:
            with OUTPUT.BoundedOutput(self.output, self.status, 4) as sink:
                raise failure
        self.assertIs(caught.exception, failure)
        self.assertFalse(caught.exception.cleanup_complete)
        with self.assertRaises(ValueError): sink.check()
        record = OUTPUT.read_status(self.status, 4, require_complete=False)
        self.assertEqual(record["error"], "cancelled")
        self.assertFalse(record["complete"])

    def test_held_open_writer_has_bounded_join_and_refuses(self):
        started = time.monotonic()
        with mock.patch.object(OUTPUT, "DRAIN_TIMEOUT_SECONDS", 0.05):
            with OUTPUT.BoundedOutput(self.output, self.status, 4) as sink:
                held = os.dup(sink.fileno())
            try:
                self.assertLess(time.monotonic() - started, 1)
                with self.assertRaises(ValueError): sink.check()
                record = OUTPUT.read_status(self.status, 4, require_complete=False)
                self.assertEqual(record["error"], "drain-timeout")
                self.assertFalse(record["complete"])
            finally:
                os.close(held)

    def test_exception_after_explicit_finish_revokes_completion(self):
        failure = OwnedProcessError("fixture", cleanup_complete=False)
        with self.assertRaises(OwnedProcessError) as caught:
            with OUTPUT.BoundedOutput(self.output, self.status, 4) as sink:
                self.assertTrue(sink.finish()["complete"])
                raise failure
        self.assertIs(caught.exception, failure)
        with self.assertRaises(ValueError): sink.check()
        record = OUTPUT.read_status(self.status, 4, require_complete=False)
        self.assertFalse(record["complete"])
        self.assertEqual(record["error"], "cancelled")

    def test_uncertain_writer_close_does_not_close_reused_descriptor(self):
        real_close = os.close
        target, failed = [None], []
        foreign = reused = None
        def uncertain_close(fd):
            real_close(fd)
            if fd == target[0] and not failed:
                failed.append(True)
                raise OSError("closed but uncertain")
        try:
            with mock.patch.object(OUTPUT.os, "close", side_effect=uncertain_close):
                with OUTPUT.BoundedOutput(self.output, self.status, 4) as sink:
                    target[0] = sink.fileno()
                    with self.assertRaises(ValueError): sink.finish()
                    foreign = os.open(self.directory / "foreign", os.O_RDWR | os.O_CREAT | os.O_EXCL, 0o600)
                    reused = os.dup2(foreign, target[0])
                    expected = os.fstat(reused).st_ino
            self.assertEqual(os.fstat(reused).st_ino, expected)
            self.assertIsNone(sink.write_fd)
            record = OUTPUT.read_status(self.status, 4, require_complete=False)
            self.assertEqual(record["error"], "input-io")
            self.assertFalse(record["complete"])
        finally:
            if reused is not None: real_close(reused)
            if foreign is not None and foreign != reused: real_close(foreign)

    def test_output_fsync_failure_is_refused(self):
        fsync = os.fsync
        with OUTPUT.BoundedOutput(self.output, self.status, 4) as sink:
            identity = sink.sink.output_identity
            def fail_output(fd):
                info = os.fstat(fd)
                if (info.st_dev, info.st_ino) == identity:
                    raise OSError("untrusted path must not enter status")
                return fsync(fd)
            with mock.patch.object(OUTPUT.os, "fsync", side_effect=fail_output):
                os.write(sink.fileno(), b"abcd")
                sink.finish(check=False)
        with self.assertRaises(ValueError): sink.check()
        record = OUTPUT.read_status(self.status, 4, require_complete=False, output=self.output)
        self.assertEqual(record["error"], "output-io")

    def test_partial_output_write_reports_exact_stored_bytes(self):
        write = os.write
        with OUTPUT.BoundedOutput(self.output, self.status, 4) as sink:
            identity = sink.sink.output_identity
            calls = []
            def partial(fd, data):
                info = os.fstat(fd)
                if (info.st_dev, info.st_ino) == identity:
                    if calls: raise OSError("fixture")
                    calls.append(True)
                    return write(fd, data[:1])
                return write(fd, data)
            with mock.patch.object(OUTPUT.os, "write", side_effect=partial):
                os.write(sink.fileno(), b"abcd")
                sink.finish(check=False)
        record = OUTPUT.read_status(self.status, 4, require_complete=False, output=self.output)
        self.assertEqual(record["stored_bytes"], 1)
        self.assertEqual(record["error"], "output-io")
        with self.assertRaises(ValueError): sink.check()

    def test_status_durability_failure_leaves_no_affirmative_evidence(self):
        with OUTPUT.BoundedOutput(self.output, self.status, 4) as sink:
            with mock.patch.object(OUTPUT, "_sync_parent", side_effect=OSError("fixture")):
                sink.finish(check=False)
        with self.assertRaises(ValueError): sink.check()
        with self.assertRaises(ValueError): OUTPUT.read_status(self.status, 4)

    def test_reader_close_failure_after_eof_still_syncs_output_and_refuses(self):
        real_thread, real_close, real_fsync = OUTPUT.threading.Thread, os.close, os.fsync
        readers = []
        def capture_thread(*args, **kwargs):
            readers.append(kwargs["args"][0])
            return real_thread(*args, **kwargs)
        with mock.patch.object(OUTPUT.threading, "Thread", side_effect=capture_thread):
            with OUTPUT.BoundedOutput(self.output, self.status, 4) as sink:
                reader, output_fd = readers[0], sink.sink.fd
                def fail_reader_close(fd):
                    real_close(fd)
                    if fd == reader:
                        raise OSError("fixture after EOF")
                with mock.patch.object(OUTPUT.os, "close", side_effect=fail_reader_close), \
                     mock.patch.object(OUTPUT.os, "fsync", side_effect=real_fsync) as synced:
                    sink.finish(check=False)
                    synced.assert_any_call(output_fd)
        with self.assertRaises(ValueError): sink.check()
        record = OUTPUT.read_status(self.status, 4, require_complete=False, output=self.output)
        self.assertEqual(record["error"], "input-io")
        self.assertFalse(record["complete"])

    def test_thread_start_failure_is_durable_refusal(self):
        with mock.patch.object(OUTPUT.threading.Thread, "start", side_effect=RuntimeError("fixture")):
            with self.assertRaises(ValueError):
                with OUTPUT.BoundedOutput(self.output, self.status, 4): pass
        record = OUTPUT.read_status(self.status, 4, require_complete=False)
        self.assertEqual(record["error"], "thread-error")
        self.assertFalse(record["complete"])

    def test_old_prepared_status_cannot_admit_new_logger(self):
        with OUTPUT.BoundedOutput(self.output, self.status, 4, owner_token="old-owner") as sink:
            old = subprocess.run([sys.executable, str(HELPER), "--check-ready", str(self.status),
                "--limit", "4", "--token", "old-owner"], capture_output=True, timeout=3)
            new = subprocess.run([sys.executable, str(HELPER), "--check-ready", str(self.status),
                "--limit", "4", "--token", "new-owner"], capture_output=True, timeout=3)
            self.assertEqual(old.returncode, 0)
            self.assertNotEqual(new.returncode, 0)
        sink.check()

    def test_exclusive_paths_symlinks_and_collision_are_refused(self):
        original = self.directory / "original"
        original.write_bytes(b"keep")
        self.output.symlink_to(original)
        with self.assertRaises(OSError):
            with OUTPUT.BoundedOutput(self.output, self.status, 4): pass
        self.assertEqual(original.read_bytes(), b"keep")
        self.output.unlink()
        with self.assertRaises(ValueError):
            with OUTPUT.BoundedOutput(self.output, self.output, 4): pass
        self.status.symlink_to(original)
        with self.assertRaises(OSError):
            with OUTPUT.BoundedOutput(self.output, self.status, 4): pass
        self.assertEqual(original.read_bytes(), b"keep")

    def test_status_tampering_and_output_replacement_fail_closed(self):
        sink = self.context_stream(b"1234", 4)
        valid = self.status.read_bytes()
        bad = json.loads(valid)
        bad["stored_bytes"] = True
        self.status.write_text(json.dumps(bad))
        with self.assertRaises(ValueError): sink.check()
        self.status.write_bytes(valid)
        self.output.unlink(); self.output.write_bytes(b"1234")
        with self.assertRaises(ValueError): sink.check()
        self.status.write_bytes(b"{" + valid[1:-2] + b',"complete":true}\n')
        with self.assertRaises(ValueError): OUTPUT.read_status(self.status, 4)
        self.status.write_bytes(b"x" * (OUTPUT.STATUS_MAX_BYTES + 1))
        with self.assertRaises(ValueError): OUTPUT.read_status(self.status, 4)


if __name__ == "__main__":
    unittest.main()
