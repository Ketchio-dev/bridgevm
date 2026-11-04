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

    def check(self, name):
        """main()'s exit code for `--name name` when the host never gets quiet."""
        spec = importlib.util.spec_from_file_location("hda_microphone_live_check", SCRIPT)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        module.CANON = self.canon
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


if __name__ == "__main__":
    unittest.main()
