#!/usr/bin/env python3
"""T17's host audio check passes typed shutdown statuses and fails anything B7 would not accept."""
from __future__ import annotations

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "scripts/live-gates"))
import t17_audio_counters  # noqa: E402

# The final host report of physical job t17-fddde07f-audio-directory-pilot-r50.
R50 = "hda CoreAudio stats: frames_rendered=206374 drops=0 dropped_bytes=0 format_drops=0 ring_full_drops=0 queue_stop_errors=0 queue_dispose_errors=0 callback_errors=3 callback_active_errors=0 callback_stopping_errors=3 callback_expected_stopping_errors=3 callback_unexpected_errors=0 callback_stopping_invalid_run_state=0 callback_stopping_queue_invalidated=0 callback_stopping_enqueue_during_reset=3 callback_stopping_disposal_pending=0 callback_stopping_unclassified=0"


class AudioCounters(unittest.TestCase):
    def test_r50_counters_pass_with_three_typed_shutdown_statuses(self) -> None:
        self.assertTrue(t17_audio_counters.passed(R50))
        self.assertIn("callback_errors=3", R50)

    def test_unexpected_dropped_or_unreconciled_counters_fail(self) -> None:
        for old, new in (("callback_unexpected_errors=0", "callback_unexpected_errors=1"), ("drops=0 ", "drops=1 "),
                         ("queue_stop_errors=0", "queue_stop_errors=1"), ("callback_errors=3", "callback_errors=4"),
                         ("callback_stopping_unclassified=0", "callback_stopping_unclassified=1")):
            self.assertFalse(t17_audio_counters.passed(R50.replace(old, new, 1)), new)

    def test_the_legacy_short_line_fails_closed(self) -> None:
        self.assertFalse(t17_audio_counters.passed("hda CoreAudio stats: frames_rendered=48000 drops=0 callback_errors=0"))


if __name__ == "__main__":
    result = unittest.main(argv=[sys.argv[0]], exit=False).result
    raise SystemExit(0 if result.wasSuccessful() else 1)
