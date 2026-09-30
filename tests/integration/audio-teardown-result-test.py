#!/usr/bin/env python3
"""B7 lane classifier cases, formerly audio-teardown-result.py --self-test.

Logs use the runtime's current layout: the NVMe write-back record, the final
report whose stop the host frames, then the CoreAudio teardown records after
its footer. hvf-stop-readers-contract.py covers guest-written copies.
"""

from __future__ import annotations

import importlib.util
from pathlib import Path
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/live-gates"))
from hvf_stop_line import SYSTEM_OFF  # noqa: E402

SPEC = importlib.util.spec_from_file_location("bridgevm_b7_lane_test", ROOT / "scripts/audio-teardown-result.py")
LANE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(LANE)
NONCE = "a" * 64
WATCHDOG = "stop: watchdog (CANCELED)"


def frame(stop: str = SYSTEM_OFF, serial: str = "") -> str:
    size = len(serial.encode())
    return (f"NVMe disk written back: fixture\n=== EDK2 boot probe (with Apple hv_gic) ===\n{stop}\n"
            f"serial raw bytes: {size} output bytes: {size}\n--- serial (tail) ---\n{serial}\n--- end ---\n"
            "hda CoreAudio lifecycle: operation=stop osstatus=0 success=true\n"
            "hda CoreAudio lifecycle: operation=dispose osstatus=0 success=true\n")


def bump(stats: str, *fields: str) -> str:
    for field in fields:
        stats = stats.replace(f" {field}=0", f" {field}=1")
    return stats


BASE = frame()
ZERO = (f"{LANE.PREFIX} frames_rendered=96000 drops=0 dropped_bytes=0 format_drops=0 "
        "ring_full_drops=0 queue_stop_errors=0 queue_dispose_errors=0 "
        "callback_errors=0 callback_active_errors=0 "
        "callback_stopping_errors=0 callback_expected_stopping_errors=0 "
        "callback_unexpected_errors=0 callback_stopping_invalid_run_state=0 "
        "callback_stopping_queue_invalidated=0 callback_stopping_enqueue_during_reset=0 "
        "callback_stopping_disposal_pending=0 callback_stopping_unclassified=0\n")
EXPECTED_EVENT = ("hda CoreAudio callback enqueue: state=stopping reason=stopping-enqueue-during-reset "
                  "osstatus=-66632 expected=true\n")
EXPECTED = bump(ZERO, "callback_errors", "callback_stopping_errors", "callback_expected_stopping_errors",
                "callback_stopping_enqueue_during_reset")
INVALIDATED_EVENT = ("hda CoreAudio callback enqueue: state=stopping reason=stopping-queue-invalidated "
                     "osstatus=-66671 expected=false\n")
INVALIDATED = bump(ZERO, "callback_errors", "callback_stopping_errors", "callback_unexpected_errors",
                   "callback_stopping_queue_invalidated")
REJECTED = {
    "stats without their event": BASE + EXPECTED,
    "active error total": BASE + bump(ZERO, "callback_active_errors"),
    "watchdog stop": frame(WATCHDOG) + ZERO,
    "guest SYSTEM_OFF after a watchdog stop": frame(WATCHDOG, f"boot\r\n{SYSTEM_OFF}\r\n") + ZERO,
    "second stats line": BASE + ZERO + ZERO,
    "false expected flag": BASE + EXPECTED_EVENT.replace("expected=true", "expected=false") + EXPECTED,
    "failed queue stop": BASE.replace("operation=stop osstatus=0 success=true",
                                      "operation=stop osstatus=-50 success=false")
    + bump(ZERO, "queue_stop_errors"),
    "queue invalidated": BASE + INVALIDATED_EVENT + INVALIDATED,
}


class AudioTeardownResultTest(unittest.TestCase):
    def validate(self, body: str, result: str = f"B7 PLAYBACK PASS nonce={NONCE} wav_bytes=384044\n") -> dict:
        with tempfile.TemporaryDirectory(prefix="bridgevm-b7-result-") as temporary:
            log, path = Path(temporary) / "run.log", Path(temporary) / "playback-result.txt"
            log.write_text(body, encoding="utf-8")
            path.write_text(result, encoding="utf-8")
            return LANE.validate(log, path, 0, NONCE, 1)

    def test_clean_lanes_pass(self):
        self.assertIs(self.validate(BASE + ZERO)["pass"], True)
        self.assertEqual(self.validate(BASE + EXPECTED_EVENT + EXPECTED)["callback_expected_stopping_errors"], 1)

    def test_invalid_lanes_fail(self):
        for name, body in REJECTED.items():
            with self.subTest(name), self.assertRaises(LANE.AudioTeardownError):
                self.validate(body)
        with self.assertRaises(LANE.AudioTeardownError):
            self.validate(BASE + ZERO, "wrong\n")


if __name__ == "__main__":
    unittest.main(verbosity=2)
