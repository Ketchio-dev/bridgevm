"""Arrange real exited/live children before asserting distinct observer failures."""
from pathlib import Path
import signal
import subprocess
import tempfile
from unittest.mock import patch

import a19_interrupt_restore_child as observer


class InterruptedChildCases:
    def test_lost_child_and_missing_stage_cannot_create_an_observation(self):
        self.observe_exited_child(delay=0)
        with tempfile.TemporaryDirectory() as temporary:
            helper, snapshot, disk, vars, output = self.make_child(Path(temporary))
            helper.write_text("#!/usr/bin/env python3\nimport time\ntime.sleep(10)\n")
            spawn = subprocess.Popen
            children = []

            def live_child(*args, **kwargs):
                child = spawn(*args, **kwargs)
                children.append(child)
                return child

            try:
                with patch.object(observer.subprocess, "Popen", side_effect=live_child):
                    with self.assertRaisesRegex(TimeoutError, r"\Ano authenticated staged read before deadline\Z"):
                        observer.run(helper, snapshot, disk, vars, output, 1)
                self.assertEqual(len(children), 1)
                self.assertEqual(children[0].returncode, -signal.SIGKILL)
                self.assert_no_observation(output)
            finally:
                # Assert production cleanup above; fallback never manufactures it.
                for child in children:
                    if child.poll() is None:
                        child.kill()
                    child.wait(timeout=5)

    def test_slow_starting_child_is_reaped_before_exit_observation(self):
        # Longer than the unchanged one-second stage-observation deadline.
        self.observe_exited_child(delay=2)

    def observe_exited_child(self, delay):
        with tempfile.TemporaryDirectory() as temporary:
            helper, snapshot, disk, vars, output = self.make_child(Path(temporary))
            helper.write_text("#!/usr/bin/env python3\nimport time\n"
                              f"time.sleep({delay})\nraise SystemExit(1)\n")
            spawn = subprocess.Popen
            children = []

            def exited_child(*args, **kwargs):
                child = spawn(*args, **kwargs)
                children.append(child)
                try:
                    self.assertEqual(child.wait(timeout=5), 1)
                except BaseException:
                    if child.poll() is None:
                        child.kill()
                    child.wait(timeout=5)
                    raise
                return child

            # Establish the intended branch with a real reaped child. Interpreter
            # startup is not required to fit inside the observation deadline.
            with patch.object(observer.subprocess, "Popen", side_effect=exited_child):
                with self.assertRaisesRegex(RuntimeError, r"\Asnapshot helper exited before staged read\Z"):
                    observer.run(helper, snapshot, disk, vars, output, 1)
            self.assertEqual(len(children), 1)
            self.assertEqual(children[0].returncode, 1)
            self.assertEqual(disk.read_bytes(), b"old-disk")
            self.assertEqual(vars.read_bytes(), b"old-vars")
            self.assert_no_observation(output)

    def assert_no_observation(self, output):
        for name in ("interrupt-helper-fd.private.log", "interrupt-helper-context.private.json",
                     "interrupt-observation.json"):
            self.assertFalse((output / name).exists(), name)
