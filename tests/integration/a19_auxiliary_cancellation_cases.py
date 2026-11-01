"""Cancellation must escape a subprocess wait's OSError fallback and run owned cleanup."""
from pathlib import Path
import signal
import tempfile
from unittest import mock

import a19_interrupt_auxiliary as auxiliary
import a19_interrupt_auxiliary_io as io


class AuxiliaryCancellationCases:
    def test_signal_cancellation_escapes_wait_oserror_fallback_and_cleans_owned_child(self):
        # CPython 3.15's subprocess._wait_kqueue catches OSError from kqueue.control,
        # including exceptions raised by a Python signal handler during that wait.
        # https://github.com/python/cpython/blob/v3.15.2/Lib/subprocess.py
        def wait_with_oserror_fallback(*, timeout):
            try:
                auxiliary.interrupted(signal.SIGTERM, None)
            except OSError:
                return 0  # The wait resumes instead of unwinding the command.

        child = mock.Mock()
        child.wait.side_effect = wait_with_oserror_fallback
        with tempfile.TemporaryDirectory() as temporary, \
                mock.patch.object(io.subprocess, "Popen", return_value=child), \
                mock.patch.object(io, "stop_group") as cleanup:
            commands = io.Commands(10)
            with self.assertRaisesRegex(auxiliary.AuxiliaryInterrupted, "auxiliary orchestration interrupted"):
                commands.run(["owned-helper"], Path(temporary) / "stdout")
            cleanup.assert_called_once_with(child)
            self.assertTrue(commands.cleanup_verified)
