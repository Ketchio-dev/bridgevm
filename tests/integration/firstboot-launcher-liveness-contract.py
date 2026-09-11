#!/usr/bin/env python3
"""Readiness distinguishes launcher exit from an elapsed readiness deadline."""
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = r'''
set -eu
source "$1/scripts/agent-channel-lib.sh"
RUN_LOG="$2/run.log"; : > "$RUN_LOG"
AGENT_TIMEOUT=30; LAUNCHER=$$; calls=0; mode="$3"; child_active=0
trap 'if [[ "$child_active" == 1 ]]; then kill "$LAUNCHER" 2>/dev/null || true; wait "$LAUNCHER" 2>/dev/null || true; fi' EXIT
sleep() { :; }
if [[ "$mode" == dead || "$mode" == dies ]]; then
  (trap - EXIT; exec /bin/sleep 30) </dev/null >/dev/null 2>&1 & LAUNCHER=$!; child_active=1
fi
if [[ "$mode" == dead ]]; then
  kill "$LAUNCHER"; wait "$LAUNCHER" 2>/dev/null || true; child_active=0
fi
send_ok() {
  calls=$((calls + 1))
  case "$mode" in
    ready) printf 'BVFIRSTBOOT_READY\n' >> "$RUN_LOG"; return 0 ;;
    dies) kill "$LAUNCHER"; wait "$LAUNCHER" 2>/dev/null || true; child_active=0 ;;
    deadline) SECONDS=$((SECONDS + AGENT_TIMEOUT + 1)) ;;
    *) echo unexpected-send >&2 ;;
  esac
  return 1
}
status=0
wait_firstboot || status=$?
printf '%s %s\n' "$status" "$calls"
'''


class FirstbootLivenessContract(unittest.TestCase):
    def test_readiness_lifetime(self):
        for mode, expected, diagnostic in (
            ("ready", "0 1", ""),
            ("dead", "2 0", "launcher exited"),
            ("dies", "2 1", "launcher exited"),
            ("deadline", "1 1", "deadline expired"),
        ):
            with self.subTest(mode=mode), tempfile.TemporaryDirectory() as directory:
                result = subprocess.run(
                    ["bash", "-c", SCRIPT, "contract", str(ROOT), directory, mode],
                    capture_output=True, text=True, timeout=5, check=True,
                )
                self.assertEqual(result.stdout.strip(), expected)
                if diagnostic:
                    self.assertIn(diagnostic, result.stderr)
                else:
                    self.assertEqual(result.stderr, "")


if __name__ == "__main__":
    unittest.main()
