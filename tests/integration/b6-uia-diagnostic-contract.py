#!/usr/bin/env python3
"""Apartment probes cannot recover a failed scene or queue after a timeout."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


class ProbeContracts(unittest.TestCase):
    def invoke(self, behavior, failure="tip-dismissal-failed"):
        with tempfile.TemporaryDirectory() as directory:
            out = Path(directory)
            (out / "run.log").touch()
            result = subprocess.run(["bash", "-c", '''
set -euo pipefail
source "$ROOT/scripts/b6-scene-contract.sh"
source "$ROOT/scripts/b6-uia-failure-probe.sh"
mhwnd=42; WIDTH=1600; HEIGHT=900; STEP_TIMEOUT=120
RUN_LOG="$OUT/run.log"
python3() { touch "$OUT/frame-observed"; }
send() {
  [[ -f "$OUT/frame-observed" && "$STEP_TIMEOUT" == 20 ]] || exit 90
  printf '%s\n' "$1" >> "$OUT/commands"
  case "$BEHAVIOR" in
    timeout) return 1 ;;
    unrelated) printf 'BVAGENT END unrelated\n' >> "$RUN_LOG" ;;
    complete) printf 'BVAGENT END %s\r\n' "$1" >> "$RUN_LOG" ;;
  esac
}
b6_scene_fail 1 packaged "$FAILURE"
touch "$OUT/continued"
'''], env={**os.environ, "ROOT": str(ROOT), "OUT": str(out),
           "BEHAVIOR": behavior, "FAILURE": failure}, capture_output=True, text=True, timeout=5)
            self.assertEqual(result.returncode, 1, result.stderr)
            self.assertFalse((out / "continued").exists())
            self.assertEqual((out / "scene-failure.env").read_text(),
                             f"run=1\nscene=packaged\nfailure_code={failure}\n")
            commands = out / "commands"
            return commands.read_text().splitlines() if commands.exists() else []

    def test_both_modes_after_frame_still_fail(self):
        commands = self.invoke("complete")
        self.assertEqual(len(commands), 3); self.assertIn("bv-b6-native-uia.ps1 -Hwnd 42", commands[2])
        self.assertIn("-Sta ", commands[0])
        self.assertIn("-Mta ", commands[1])
        self.assertTrue(all("-File C:\\BridgeVMClosure\\bv-b6-uia-diagnostic.ps1 -Hwnd 42" in c for c in commands[:2]))

    def test_timeout_stops_further_queries(self):
        self.assertEqual(len(self.invoke("timeout")), 1)

    def test_unrelated_completion_stops_further_queries(self):
        self.assertEqual(len(self.invoke("unrelated")), 1)

    def test_other_failure_does_not_query(self):
        self.assertEqual(self.invoke("complete", "reset-failed"), [])

    def test_guest_asset_is_crlf(self):
        data = (ROOT / "scripts/win-assets/bv-b6-uia-diagnostic.ps1").read_bytes()
        self.assertIn(b"\r\n", data)
        self.assertNotIn(b"\n", data.replace(b"\r\n", b""))


if __name__ == "__main__":
    unittest.main()
