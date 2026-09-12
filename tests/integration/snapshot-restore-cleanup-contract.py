#!/usr/bin/env python3
"""Restore gate rejects residual children and preserves uncertain work files."""
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


class RestoreCleanupContract(unittest.TestCase):
    def run_shell(self, body):
        with tempfile.TemporaryDirectory() as directory:
            script = 'source "$1/scripts/snapshot-restore-lifecycle.sh"\n' + body
            subprocess.run(["bash", "-c", script, "contract", str(ROOT), directory],
                           check=True, timeout=20, capture_output=True, text=True)

    def test_residual_group_cannot_pass_after_successful_cleanup(self):
        self.run_shell('''
STEP_TIMEOUT=2
printf 'stop: PSCI x (system off)\nNVMe disk written back: x\n' > "$2/log"
(exit 0) & SNAPSHOT_LAUNCHER=$!
bridgevm_process_group_alive() { return 0; }
cleaned=no
snapshot_stop_launcher() { cleaned=yes; SNAPSHOT_LAUNCHER=""; }
if snapshot_shutdown "$2/ctl" "$2/log"; then exit 8; fi
[[ "$cleaned" == yes && $(cat "$2/ctl") == 'shutdown.exe /s /t 0' ]]
''')

    def test_unconfirmed_cleanup_preserves_work(self):
        self.run_shell('''
WORK="$2/work"; mkdir "$WORK"; printf retained > "$WORK/disk.raw"
snapshot_stop_launcher() { return 1; }
if snapshot_cleanup; then exit 8; fi
[[ $(cat "$WORK/disk.raw") == retained ]]
''')

    def test_owned_group_cleanup_stops_descendants_not_other_groups(self):
        self.run_shell('''
sleep 30 & unrelated=$!
trap 'kill "$unrelated" 2>/dev/null; wait "$unrelated" 2>/dev/null' EXIT
(
  set +m
  sleep 30 & child=$!
  trap 'kill "$child" 2>/dev/null; wait "$child" 2>/dev/null; exit 0' TERM
  printf '%s\n' "$child" > "$2/child"
  wait "$child"
) & SNAPSHOT_LAUNCHER=$!
for ((i=0; i<100; i++)); do [[ -s "$2/child" ]] && break; sleep 0.02; done
[[ -s "$2/child" ]] || exit 9
child=$(cat "$2/child")
snapshot_stop_launcher || exit 8
kill -0 "$unrelated" || exit 7
if bridgevm_process_alive "$child"; then exit 6; fi
[[ -z "$SNAPSHOT_LAUNCHER" ]]
''')


if __name__ == "__main__":
    unittest.main()
