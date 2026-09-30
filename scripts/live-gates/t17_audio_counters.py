"""T17's host audio check on B7's proven counter semantics.

It parses with scripts/audio-teardown-result.py, so the exact ordered field set
and every reconciliation are B7's own. Shutdown statuses the host types as
expected are counted rather than failed; anything unexpected, dropped or
unreconciled fails. The Swift twin is apps/macos/Sources/BridgeVMProductE2E/T17AudioCounters.swift.
"""
from __future__ import annotations

import importlib.util
from pathlib import Path

_SPEC = importlib.util.spec_from_file_location(
    "bridgevm_audio_teardown_result", Path(__file__).resolve().parent.parent / "audio-teardown-result.py")
_B7 = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(_B7)


def passed(line: str) -> bool:
    try:
        values = _B7.parse_stats(line)
    except _B7.AudioTeardownError:
        return False
    return (values["frames_rendered"] > 0 and values["drops"] == 0 and values["queue_stop_errors"] == 0
            and values["queue_dispose_errors"] == 0 and values["callback_unexpected_errors"] == 0
            and values["callback_errors"] == values["callback_expected_stopping_errors"])
