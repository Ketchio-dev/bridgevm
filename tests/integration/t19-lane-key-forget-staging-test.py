#!/usr/bin/env python3
"""The T19 tier stages a lane's sealed inputs before it writes the lane's request, and no app runs for the lane
until the request exists. A cancel or failure in that window leaves a lane holding only inputs/, which can own no
imported key, so cleanup accounts for it without an app call. Anything more without a request is still refused."""
from __future__ import annotations

import importlib.util
import os
from pathlib import Path
import shutil
import subprocess
import sys
import unittest

BASE_PATH = Path(__file__).resolve().parent / "t19-lane-key-forget-test.py"
SPEC = importlib.util.spec_from_file_location("t19_lane_key_forget_test", BASE_PATH)
BASE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BASE)


class StagedLaneTest(BASE.LaneKeyForgetTest):
    def stage(self, number: int) -> Path:
        """Stage a lane's inputs exactly as the tier does before it writes the request."""
        root, inputs = self.work / f"lane-{number}", self.work / f"lane-{number}/inputs"
        root.mkdir(mode=0o700)
        (inputs / "vtpm").mkdir(mode=0o700, parents=True)
        for name in ("windows.raw", "vars.fd", "vtpm-recovery.json", "vtpm-recovery-code.txt"):
            shutil.copy2(self.source / name, inputs / name)
        subprocess.run(["chmod", "-R", "a-w", str(inputs)], check=True)
        return root

    def test_a_lane_staged_before_its_request_needs_no_app_call(self) -> None:
        self.stage(1)
        result = self.forget()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.calls(), [])

    def test_a_staged_release_lane_after_a_finished_lane_needs_no_app_call(self) -> None:
        self.seal("release")
        request = self.lane(1)
        self.imported(request["vm_slug"])
        self.stage(2)
        result = self.forget()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual([call["--stable-vm-id"] for call in self.calls()], [request["vm_slug"]])
        self.assertFalse(self.held(request["vm_slug"]), "the finished lane's imported key survived cleanup")

    def test_a_lane_without_a_request_holding_more_than_inputs_is_refused(self) -> None:
        root = self.stage(1)
        (root / "library").mkdir()
        self.assert_refused(self.forget(), [], "lane 1 holds more than its staged inputs but no request")

    def test_linked_inputs_without_a_request_are_refused(self) -> None:
        root = self.work / "lane-1"
        root.mkdir(mode=0o700)
        os.symlink(self.source, root / "inputs")
        self.assert_refused(self.forget(), [], "lane 1 holds more than its staged inputs but no request")


def load_tests(loader: unittest.TestLoader, tests: unittest.TestSuite, pattern: str | None) -> unittest.TestSuite:
    """Only this file's cases; the inherited harness's own cases run from their own file."""
    names = [name for name in vars(StagedLaneTest) if name.startswith("test")]
    return unittest.TestSuite(StagedLaneTest(name) for name in sorted(names))


if __name__ == "__main__":
    sys.exit(0 if unittest.main(exit=False).result.wasSuccessful() else 1)
