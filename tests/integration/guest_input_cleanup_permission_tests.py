"""Permission transitions never establish cleanup without actual group absence."""
import errno
import os
from pathlib import Path
import signal
import tempfile
import unittest
from unittest.mock import Mock, call, patch

import guest_input_live_cleanup as cleanup


class PermissionCleanup(unittest.TestCase):
    def process(self):
        return Mock(pid=os.getpgrp() + 100)

    def test_permission_denial_is_potentially_alive(self):
        with patch.object(cleanup.os, "killpg", side_effect=PermissionError()):
            self.assertTrue(cleanup.group_alive(self.process().pid))

    def test_persistent_denial_cannot_complete_cleanup(self):
        process = self.process()
        with patch.object(cleanup.os, "killpg", side_effect=PermissionError()) as send:
            self.assertFalse(cleanup.stop(process, grace=0))
        self.assertEqual(send.call_args_list, [call(process.pid, signal.SIGTERM),
                         call(process.pid, signal.SIGKILL), call(process.pid, 0)])

    def test_probe_denial_then_absence_completes(self):
        process = self.process()
        with patch.object(cleanup.os, "killpg", side_effect=[None, PermissionError(),
                          ProcessLookupError()]) as send:
            self.assertTrue(cleanup.stop(process, grace=1))
        self.assertEqual(send.call_args_list, [call(process.pid, signal.SIGTERM),
                         call(process.pid, 0), call(process.pid, 0)])

    def test_signal_denial_still_requires_absence_probe(self):
        process = self.process()
        with patch.object(cleanup.os, "killpg", side_effect=[PermissionError(),
                          ProcessLookupError()]) as send:
            self.assertTrue(cleanup.stop(process, grace=1))
        self.assertEqual(send.call_args_list, [call(process.pid, signal.SIGTERM),
                                             call(process.pid, 0)])

    def test_unexpected_os_errors_are_not_swallowed(self):
        with patch.object(cleanup.os, "killpg", side_effect=OSError(errno.EIO, "io")):
            with self.assertRaises(OSError):
                cleanup.group_alive(self.process().pid)
            with self.assertRaises(OSError):
                cleanup.stop(self.process(), grace=0)

    def test_denied_cleanup_never_touches_disk(self):
        process, disk = self.process(), Mock()
        with tempfile.TemporaryDirectory() as tmp:
            receipt = {"claim_eligible": False}
            with patch.object(cleanup.os, "killpg", side_effect=PermissionError()), \
                 patch.object(cleanup.time, "monotonic", side_effect=range(100)), \
                 patch.object(cleanup.time, "sleep"), patch.object(cleanup, "digest") as digest:
                cleanup.finalize(receipt, process, {}, {}, {"disk": disk}, Path(tmp))
            self.assertFalse(receipt["cleanup_complete"])
            self.assertFalse(receipt["complete"])
            self.assertFalse(receipt["claim_eligible"])
            digest.assert_not_called()
            disk.chmod.assert_not_called()
