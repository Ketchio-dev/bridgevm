#!/usr/bin/env python3
"""A5's verifier takes the CoreAudio counters only from the final host report.

verify-audio-playback.sh hands audio-playback-result.py the record its STATS
line selects. The host prints its counters after the final report's footer; a
guest can print the same text as agent output or in the counted serial tail.
The forged logs of hvf-stop-readers-contract.py carry passing counters the host
never printed, or have no host report at all.
"""

from __future__ import annotations

import importlib.util
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
VERIFIER = ROOT / "scripts/verify-audio-playback.sh"


def load(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


READERS = load("hvf_stop_readers_contract", ROOT / "tests/integration/hvf-stop-readers-contract.py")
CLASSIFIER = load("bridgevm_a5_classifier", ROOT / "scripts/audio-playback-result.py")
FORGED = {**READERS.FORGED_AUDIO, "no host report": READERS.FORGED_STOP["record without a host report"]}


class AudioPlaybackHostStatsContract(unittest.TestCase):
    def passes(self, run_log: Path) -> bool:
        """The verifier's own STATS line over run_log, then its classifier with a clean launcher exit."""
        [line] = [line for line in VERIFIER.read_text(encoding="utf-8").splitlines() if line.startswith("STATS=")]
        stats = subprocess.run(["bash", "-c", f'set -euo pipefail; RUN_LOG="$1"; {line}; printf %s "$STATS"', "a5",
                                str(run_log)], cwd=ROOT, check=True, capture_output=True, text=True, timeout=30).stdout
        try:
            return CLASSIFIER.validate_result(0, stats) == (96000, 0, 0)
        except CLASSIFIER.AudioResultError:
            return False

    def test_counters_come_from_the_host_tail(self):
        with tempfile.TemporaryDirectory() as temp:
            log, genuine = Path(temp) / "run.log", Path(temp) / "genuine.log"
            genuine.write_bytes(READERS.GENUINE.encode())
            log.write_bytes(READERS.GENUINE.encode())
            self.assertTrue(self.passes(log))
            accepted = []
            for name, text in FORGED.items():
                log.write_bytes(text.encode())
                accepted += [name] if self.passes(log) else []
            log.unlink()
            log.symlink_to(genuine)
            accepted += ["run.log linked to a genuine log"] if self.passes(log) else []
            log.unlink()
            os.mkfifo(log)
            accepted += ["run.log is a FIFO"] if self.passes(log) else []
            self.assertEqual(accepted, [])


if __name__ == "__main__":
    unittest.main(verbosity=2)
