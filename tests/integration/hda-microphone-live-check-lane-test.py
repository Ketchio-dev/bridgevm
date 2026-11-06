#!/usr/bin/env python3
"""The microphone live check deletes only a lane it created.

Each case runs scripts/live-gates/hda-microphone-live-check.py's main() against a
temporary perf home with the quiet-host wait timing out, so the run ends before it
creates anything (and never starts a sentinel), and checks that a canonical-image
placeholder, another run's lane and a directory outside the perf home survive.
"""
import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

SCRIPT = Path(__file__).resolve().parents[2] / "scripts/live-gates/hda-microphone-live-check.py"


class LaneCleanup(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="bridgevm-mic-lane-")
        self.root = Path(self.tmp.name)
        self.canon = self.root / ".bvm-perf-internal"
        self.kept = [self.canon / "canonical.raw", self.canon / "lanes" / "taken" / "keep",
                     self.root / "victim" / "keep"]
        for path in self.kept:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("must survive")
        self.probe = self.root / "probe"
        self.probe.write_bytes(b"not run")
        self.runs = 0

    def tearDown(self):
        self.tmp.cleanup()

    def load(self):
        spec = importlib.util.spec_from_file_location("hda_microphone_live_check", SCRIPT)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        module.CANON = self.canon
        return module

    def check(self, name, module=None):
        """main()'s exit code for `--name name` when the host never gets quiet."""
        if module is None:
            module = self.load()
            module.wait_for_quiet_host = lambda *args, **kwargs: False
        self.runs += 1
        argv, sys.argv = sys.argv, ["hda-microphone-live-check.py", "--probe", str(self.probe),
                                    "--out", str(self.root / f"out-{self.runs}"), "--name", name]
        try:
            return module.main()
        except SystemExit as exit:
            return exit.code
        finally:
            sys.argv = argv

    def assert_kept(self):
        for path in self.kept:
            self.assertTrue(path.is_file(), f"{path.relative_to(self.root)} was deleted")

    def test_a_name_that_leaves_the_lanes_directory_is_refused(self):
        for name in ["..", ".", "", "../../victim", str(self.root / "victim"), "a/b"]:
            with self.subTest(name=name):
                self.assertNotEqual(self.check(name), 0)
                self.assert_kept()

    def test_a_lane_this_run_did_not_create_is_left_alone(self):
        self.assertNotEqual(self.check("taken"), 0)
        self.assert_kept()
        self.assertNotEqual(self.check("fresh"), 0)
        self.assertFalse((self.canon / "lanes" / "fresh").exists(), "a wait that times out creates no lane")

    def test_ctrl_c_after_the_boot_stops_the_runner_before_the_lane_goes(self):
        module = self.load()
        module.wait_for_quiet_host = lambda *args, **kwargs: True
        module.other_vms = lambda own=(): []
        lane = self.canon / "lanes" / "interrupted"
        events = []

        class Proc:
            def __init__(self, pid):
                self.pid, self.returncode = pid, None

            def poll(self):
                return self.returncode

            def wait(self, timeout=None):
                events.append(("wait", self.pid, lane.exists()))
                self.returncode = 0
                return 0

            def kill(self):
                self.returncode = -9

        procs = iter([Proc(100), Proc(200)])  # the sentinel, then the runner

        def interrupt(*args, **kwargs):
            raise KeyboardInterrupt

        def run(argv, **kwargs):  # git, codesign and cp -c; cp leaves an empty clone
            if argv[0] == "cp":
                Path(argv[-1]).write_bytes(b"")
            return mock.Mock(stdout="", returncode=0)

        with mock.patch.object(module.subprocess, "run", run), \
                mock.patch.object(module.subprocess, "Popen", lambda *a, **k: next(procs)), \
                mock.patch.object(module.os, "killpg", lambda pid, sig: events.append(("kill", pid, lane.exists()))), \
                mock.patch.object(module.Guest, "wait_for", interrupt):
            with self.assertRaises(KeyboardInterrupt):
                self.check("interrupted", module)
        self.assertIn(("kill", 200, True), events, "the runner was not signalled while its lane existed")
        self.assertIn(("wait", 200, True), events, "the runner was not reaped while its lane existed")
        self.assertFalse(lane.exists())
        self.assert_kept()


if __name__ == "__main__":
    unittest.main()
