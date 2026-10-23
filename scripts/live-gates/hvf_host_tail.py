"""The host records that may follow the final HVF report's footer.

After final_report.rs prints `--- end ---` only host teardown records follow:
the HDA controller's diagnostic records, at most once each in print order
(hvf_host_diagnostics.py), then CoreAudio callback-enqueue, lifecycle and stats
records, at most one CoreAudio continuity record after a stats record
(hda_coreaudio_continuity.rs), and with 3D the render server's disconnect line.
All count against MAX_AUDIO_TAIL_LINES records and MAX_AUDIO_TAIL_BYTES bytes,
so a guest cannot extend a forged footer. A log printed before either record
kind existed reads exactly as before; nothing requires one.
t17_terminal_report_tail applies AUDIO_LINES and hvf_terminal_report HOST_TAIL;
scripts/hvf-terminal-report.sh and apps/macos/.../HvfHostTail.swift mirror both.
"""

from __future__ import annotations

import re
from hvf_host_diagnostics import DIAGNOSTIC_CODES, DIAGNOSTIC_LINES, DIAGNOSTIC_ORDER

MAX_AUDIO_TAIL_BYTES = 4 * 1024
MAX_AUDIO_TAIL_LINES = 16
STATS_PREFIX = "hda CoreAudio stats:"
CONTINUITY_PREFIX = "hda CoreAudio continuity:"
CONTINUITY_FIELDS = ("active_callbacks", "underrun_callbacks", "underrun_frames", "contention_callbacks", "gaps",
                     "max_gap_frames", "stream_stops", "callback_frames")
CONTINUITY = re.compile(CONTINUITY_PREFIX + "".join(f" {field}=(0|[1-9][0-9]{{0,19}})" for field in CONTINUITY_FIELDS))
AUDIO_LINES = (
    re.compile(r"hda CoreAudio callback enqueue: state=stopping reason=[a-z-]+ osstatus=-?[0-9]+ expected=(true|false)"),
    re.compile(r"hda CoreAudio lifecycle: operation=(stop|dispose) osstatus=-?[0-9]+ success=(true|false)"),
    re.compile(r"hda CoreAudio stats: [a-z][a-z0-9_]*=[0-9]+( [a-z][a-z0-9_]*=[0-9]+)*"),
    CONTINUITY, *DIAGNOSTIC_LINES,
)
HOST_TAIL = (*AUDIO_LINES, re.compile(r"[A-Z][a-z]{2} [ 0-9][0-9] [0-9]{2}:[0-9]{2}:[0-9]{2}  "
                                      r"virgl_render_server\[[0-9]+\] <Debug>: socket disconnected"))
_CODES = {STATS_PREFIX: "S", CONTINUITY_PREFIX: "C", **DIAGNOSTIC_CODES}
_PLACED = re.compile(DIAGNOSTIC_ORDER + "(S+C)?S*")


def continuity_placed(records: list[str]) -> bool:
    """At most one continuity record, only after a stats record; diagnostic records before both."""
    order = "".join(code for record in records for prefix, code in _CODES.items() if record.startswith(prefix))
    return _PLACED.fullmatch(order) is not None


def bounded_host_shutdown_tail(tail: str) -> bool:
    """A T17 terminal report's post-footer text: empty, or bounded audio records."""
    if not tail:
        return True
    if len(tail.encode("utf-8")) > MAX_AUDIO_TAIL_BYTES or not tail.endswith("\n"):
        return False
    lines = tail[:-1].split("\n")
    return 1 <= len(lines) <= MAX_AUDIO_TAIL_LINES and continuity_placed(lines) and all(
        any(pattern.fullmatch(line) for pattern in AUDIO_LINES) for line in lines
    )
