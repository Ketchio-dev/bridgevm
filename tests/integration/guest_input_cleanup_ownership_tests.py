"""Process-group PID reservations; disposable owned fixtures never start a VM."""
import os
from contextlib import contextmanager
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import Mock, patch

import guest_input_live_cleanup as cleanup
import guest_input_owned_group as owned_group


def unreaped_process():
    process = Mock(pid=os.getpgrp() + 100, returncode=None)
    def reap(timeout): process.returncode = 0; return 0
    process.wait.side_effect = reap
    return process


@contextmanager
def reaped_group_fixture():
    # Both handles belong to this fixture. A fresh group in our session lets
    # its owner kill and reap the residual child after a refused cleanup.
    leader = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(15)"],
                              preexec_fn=os.setpgrp)
    child = None
    try:
        child = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(15)"],
                                 preexec_fn=lambda: os.setpgid(0, leader.pid))
        leader.terminate()
        leader.wait(timeout=5)
        yield leader, child
    finally:
        for owned in (child, leader):
            if owned is not None:
                if owned.poll() is None: owned.kill()
                owned.wait(timeout=5)


class OwnershipCleanup(unittest.TestCase):
    def record(self, process, trace):
        original = os.killpg
        def send(pgid, sig):
            self.assertEqual(pgid, process.pid)
            trace.append((sig, process.returncode))
            return original(pgid, sig)
        return send

    def test_reaped_absent_session_has_only_zero_signal(self):
        process = subprocess.Popen([sys.executable, "-c", "pass"], start_new_session=True)
        process.wait(timeout=5)
        trace = []
        with patch.object(owned_group.os, "killpg", side_effect=self.record(process, trace)):
            self.assertTrue(cleanup.stop(process, grace=.02))
        self.assertTrue(trace)
        self.assertTrue(all(sig == 0 for sig, _ in trace), trace)

    def test_reaped_residual_group_is_fenced_without_signals(self):
        with reaped_group_fixture() as (leader, child):
            trace = []
            with patch.object(owned_group.os, "killpg", side_effect=self.record(leader, trace)):
                self.assertFalse(cleanup.stop(leader, grace=.02))
            self.assertTrue(all(sig == 0 for sig, _ in trace), trace)
            self.assertIsNone(child.poll())

    def test_actual_residual_finalizer_retains_media_without_hash_or_chmod(self):
        with reaped_group_fixture() as (leader, child), tempfile.TemporaryDirectory() as tmp:
            receipt, disk = {"claim_eligible": False}, Mock()
            with patch.object(cleanup, "digest") as digest:
                cleanup.finalize(receipt, leader, {}, {}, {"image": disk}, Path(tmp))
            self.assertFalse(receipt["cleanup_complete"])
            self.assertFalse(receipt["complete"])
            digest.assert_not_called()
            disk.chmod.assert_not_called()
            self.assertIsNone(child.poll())

    def test_reaped_denied_group_cannot_complete(self):
        process = Mock(pid=os.getpgrp() + 100, returncode=0)
        with patch.object(owned_group.os, "killpg", side_effect=PermissionError()) as send:
            self.assertFalse(cleanup.stop(process, grace=0))
        self.assertEqual(send.call_args_list[0].args, (process.pid, 0))
        self.assertEqual(send.call_count, 1)
        process.wait.assert_not_called()
        process.poll.assert_not_called()

    def test_kill_occurs_before_leader_reap(self):
        trace, killed = [], []
        class Process:
            pid = os.getpgrp() + 100
            returncode = None
            def poll(self): self.returncode = 0; return 0
            def wait(self, timeout): self.returncode = -9; return -9
        process = Process()
        def send(pgid, sig):
            trace.append((sig, process.returncode))
            if sig == signal.SIGKILL: killed.append(True)
            if sig == 0 and killed and process.returncode is not None:
                raise ProcessLookupError()
        with patch.object(owned_group.os, "killpg", side_effect=send):
            self.assertTrue(cleanup.stop(process, grace=.001))
        self.assertTrue(killed)
        self.assertTrue(all(code is None for sig, code in trace if sig != 0), trace)

    def test_reap_during_term_prevents_later_nonzero_signals(self):
        for absent in (True, False):
            with self.subTest(absent=absent):
                process, trace = unreaped_process(), []
                def send(pgid, sig):
                    trace.append(sig)
                    if sig == signal.SIGTERM: process.returncode = 0
                    if sig == 0 and absent: raise ProcessLookupError()
                with patch.object(owned_group.os, "killpg", side_effect=send):
                    self.assertIs(cleanup.stop(process, grace=.001), absent)
                self.assertEqual(trace, [signal.SIGTERM, 0])

    def test_reap_during_grace_prevents_final_kill(self):
        for absent in (True, False):
            with self.subTest(absent=absent):
                process, trace = unreaped_process(), []
                def send(pgid, sig):
                    trace.append(sig)
                    if sig == 0:
                        if process.returncode is not None and absent: raise ProcessLookupError()
                        process.returncode = 0
                with patch.object(owned_group.os, "killpg", side_effect=send):
                    self.assertIs(cleanup.stop(process, grace=.001), absent)
                self.assertEqual(trace, [signal.SIGTERM, signal.SIGCONT, 0, 0])

    def test_zombie_style_permission_requires_owned_reap_and_absence(self):
        process = Mock(pid=os.getpgrp() + 100, returncode=None)
        def reap(timeout): process.returncode = 0; return 0
        process.wait.side_effect = reap
        with patch.object(owned_group.os, "killpg", side_effect=[PermissionError(), ProcessLookupError()]) as send:
            self.assertTrue(cleanup.stop(process, grace=.02))
        process.wait.assert_called_once()
        self.assertEqual([call.args[1] for call in send.call_args_list], [signal.SIGTERM, 0])
        process.poll.assert_not_called()

    def test_permission_then_reaped_residual_stays_fenced(self):
        process = Mock(pid=os.getpgrp() + 100, returncode=None)
        def reap(timeout): process.returncode = 0; return 0
        process.wait.side_effect = reap
        with patch.object(owned_group.os, "killpg", side_effect=[PermissionError(), None]) as send:
            self.assertFalse(cleanup.stop(process, grace=.02))
        self.assertEqual([call.args[1] for call in send.call_args_list], [signal.SIGTERM, 0])

    def owned(self, program):
        process = subprocess.Popen([sys.executable, "-c", program], start_new_session=True)
        self.addCleanup(self.finish, process)
        return process

    def finish(self, process):
        # Fixture-only cleanup of the exact retained child handle.
        if process.poll() is None: process.kill()
        process.wait(timeout=5)

    def ready(self, path, process):
        deadline = time.monotonic() + 3
        while not path.exists() and time.monotonic() < deadline:
            self.assertIsNone(process.poll())
            time.sleep(.01)
        self.assertTrue(path.is_file())

    def test_stopped_owned_leader_is_continued_and_reaped(self):
        process = self.owned("import time; time.sleep(15)")
        process.send_signal(signal.SIGSTOP)
        trace = []
        with patch.object(owned_group.os, "killpg", side_effect=self.record(process, trace)):
            self.assertTrue(cleanup.stop(process, grace=.05))
        self.assertIsNotNone(process.returncode)
        self.assertIn(signal.SIGCONT, [sig for sig, _ in trace])
        self.assertTrue(all(code is None for sig, code in trace if sig != 0), trace)

    def test_term_ignoring_owned_leader_is_killed_then_reaped(self):
        with tempfile.TemporaryDirectory() as tmp:
            ready = Path(tmp) / "ready"
            program = ("import signal,time; from pathlib import Path; "
                       "signal.signal(signal.SIGTERM,signal.SIG_IGN); "
                       "Path(" + repr(str(ready)) + ").touch(); time.sleep(15)")
            process = self.owned(program)
            self.ready(ready, process)
            trace = []
            with patch.object(owned_group.os, "killpg", side_effect=self.record(process, trace)):
                self.assertTrue(cleanup.stop(process, grace=.05))
            self.assertEqual(process.returncode, -signal.SIGKILL)
            self.assertTrue(all(code is None for sig, code in trace if sig != 0), trace)

    def test_term_tree_handler_reaps_its_owned_child(self):
        with tempfile.TemporaryDirectory() as tmp:
            ready = Path(tmp) / "ready"
            program = ("import signal,subprocess,sys,time; from pathlib import Path\n"
                       "child=subprocess.Popen([sys.executable,'-c','import time;time.sleep(15)'])\n"
                       "def stop(sig,frame):\n child.wait(timeout=2)\n sys.exit(0)\n"
                       "signal.signal(signal.SIGTERM,stop)\nPath(" + repr(str(ready))
                       + ").touch()\ntime.sleep(15)\n")
            process = self.owned(program)
            self.ready(ready, process)
            self.assertTrue(cleanup.stop(process, grace=.1))
            self.assertEqual(process.returncode, 0)
