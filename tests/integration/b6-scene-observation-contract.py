#!/usr/bin/env python3
"""Failure diagnostics remain bounded and cannot manufacture gate evidence."""
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("observer", ROOT / "scripts/b6-observe-scene-failure.py")
OBSERVER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(OBSERVER)


class ObservationTests(unittest.TestCase):
    def test_missing_surface_is_not_a_capture(self):
        with tempfile.TemporaryDirectory() as directory, patch.object(OBSERVER.subprocess, "run") as run:
            out = Path(directory)
            result = OBSERVER.observe(out)
            run.assert_not_called()
            self.assertEqual(result["status"], "surface-unavailable")
            self.assertTrue(result["observation_only"])
            self.assertFalse(result["criterion_pass"])
            self.assertFalse(result["freshness_verified"])
            self.assertFalse((out / "scene-failure-observed/observed.ppm").exists())

    def test_child_outcomes_remain_observations(self):
        cases = [(0, "observer-returned"), (1, "observer-failed"),
                 (subprocess.TimeoutExpired("observer", 10), "observer-timeout"),
                 (OSError(2, "missing"), "observer-unavailable")]
        for outcome, status in cases:
            with self.subTest(status=status), tempfile.TemporaryDirectory() as directory:
                out = Path(directory)
                (out / "display.fb.iosurface").write_text("123")
                with patch.object(OBSERVER.subprocess, "run") as run:
                    if isinstance(outcome, Exception):
                        run.side_effect = outcome
                    else:
                        run.return_value = subprocess.CompletedProcess([], outcome)
                    result = OBSERVER.observe(out)
                self.assertEqual(result["status"], status)
                self.assertEqual(run.call_args.kwargs["timeout"], 10)
                self.assertIn("observe-active-iosurface.py", run.call_args.args[0][1])
                self.assertFalse(result["criterion_pass"])
                self.assertFalse(result["freshness_verified"])
                self.assertEqual(json.loads((out / "scene-failure-observed/observation.json").read_text()), result)

    def test_observer_failure_cannot_continue_scene(self):
        with tempfile.TemporaryDirectory() as directory:
            out = Path(directory)
            result = subprocess.run(
                ["bash", "-c", 'set -euo pipefail; source "$1"; OUT="$2"; '
                 'python3() { return 71; }; b6_scene_fail 2 packaged tip-dismissal-failed; '
                 'touch "$OUT/continued"', "test", str(ROOT / "scripts/b6-scene-contract.sh"), str(out)],
                capture_output=True, text=True, timeout=5)
            self.assertEqual(result.returncode, 1)
            self.assertFalse((out / "continued").exists())
            self.assertEqual((out / "scene-failure.env").read_text(),
                             "run=2\nscene=packaged\nfailure_code=tip-dismissal-failed\n")


if __name__ == "__main__":
    unittest.main()
