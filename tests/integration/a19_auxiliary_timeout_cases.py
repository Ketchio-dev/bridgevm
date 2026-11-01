"""Timeout cleanup runs against a ready owned process tree, independent of interpreter startup latency."""
from pathlib import Path
import os
import subprocess
import sys
import tempfile
import time
from unittest import mock

import a19_interrupt_auxiliary_io as io


class AuxiliaryTimeoutCases:
    def test_timeout_kills_and_reaps_an_owned_command_tree(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            script, pid_path = root / "tree.py", root / "descendant.pid"
            script.write_text("import pathlib,signal,subprocess,time\n"
                              "child=subprocess.Popen(['/bin/sleep','30'])\n"
                              "def stop(signum,frame):\n"
                              "    child.terminate();child.wait(timeout=1);raise SystemExit(0)\n"
                              "signal.signal(signal.SIGTERM,stop)\n"
                              f"pathlib.Path({str(pid_path)!r}).write_text(str(child.pid))\n"
                              "time.sleep(30)\n")
            popen = subprocess.Popen

            def ready_child(*args, **kwargs):
                child = popen(*args, **kwargs)
                try:
                    until = time.monotonic() + 3
                    while not pid_path.exists() and child.poll() is None and time.monotonic() < until:
                        time.sleep(0.02)
                    self.assertTrue(pid_path.exists(), "fixture command tree never became ready")
                except BaseException:
                    io.stop_group(child)
                    raise
                return child

            commands = io.Commands(5, command_timeout=0.5)
            with mock.patch.object(io.subprocess, "Popen", side_effect=ready_child):
                with self.assertRaises(subprocess.TimeoutExpired):
                    commands.run([sys.executable, str(script)], root / "tree.stdout")
            self.assertTrue(commands.cleanup_verified)
            with self.assertRaises(ProcessLookupError):
                os.kill(int(pid_path.read_text()), 0)
