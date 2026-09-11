#!/usr/bin/env python3
"""A command exit record cannot replace the channel's END acknowledgment."""
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


class AcknowledgmentContract(unittest.TestCase):
    def test_conditional_callers_require_acknowledgment_and_fresh_success(self):
        cases = (
            (1, "BVAGENT CMD diagnostic exit=0", False),
            (1, "", False),
            (0, "BVAGENT CMD diagnostic exit=0", True),
            (0, "BVAGENT CMD diagnostic exit=5", False),
            (0, "", False),
        )
        for context in ("if", "and"):
            for send_status, record, expected in cases:
                with self.subTest(context=context, send_status=send_status, record=record):
                    with tempfile.TemporaryDirectory() as directory:
                        script = r'''
set -eu
source "$1/scripts/agent-channel-lib.sh"
RUN_LOG="$2/run.log"
printf 'BVAGENT CMD stale exit=0\n' > "$RUN_LOG"
send() { if [[ -n "$4" ]]; then printf '%s\n' "$4" >> "$RUN_LOG"; fi; return "$3"; }
# Bind fixture arguments outside send's own positional arguments.
fixture_status="$3"; fixture_record="$4"
send() { [[ -z "$fixture_record" ]] || printf '%s\n' "$fixture_record" >> "$RUN_LOG"; return "$fixture_status"; }
if [[ "$5" == if ]]; then
  if send_ok diagnostic; then printf accepted; else printf rejected; fi
else
  result=rejected; send_ok diagnostic && result=accepted; printf '%s' "$result"
fi
'''
                        result = subprocess.run(
                            ["bash", "-c", script, "contract", str(ROOT), directory,
                             str(send_status), record, context],
                            capture_output=True, text=True, timeout=5, check=True)
                        self.assertEqual(result.stdout, "accepted" if expected else "rejected")


if __name__ == "__main__":
    unittest.main()
