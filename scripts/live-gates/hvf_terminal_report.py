"""The HVF final report's stop record, bound to host framing rather than text.

final_report.rs prints the banner, `stop: {reason}`, host records, the count
`serial raw bytes: R output bytes: M`, `--- serial (tail) ---`, exactly M bytes
of rendered guest serial and the `--- end ---` footer; only host teardown
records follow. Guest bytes can precede the report and fill the counted tail,
so the stop record follows the last banner before the only count whose tail
ends at the final footer; a second one, which a guest can nest, rejects the log.
COUNT_NEW's 8-digit fields make that hold only below LOG_LIMIT bytes and only
for the whole run.log, read after the helper exited: a window can drop the count.
"""

from __future__ import annotations

import re
from hvf_stop_line import SYSTEM_OFF
from t17_terminal_report_tail import (AUDIO_LINES, BANNER, COUNT_NEW, FOOTER, MAX_AUDIO_TAIL_BYTES,
                                      MAX_AUDIO_TAIL_LINES, SERIAL)

LOG_LIMIT = 10**8
HOST_TAIL = (*AUDIO_LINES, re.compile(r"[A-Z][a-z]{2} [ 0-9][0-9] [0-9]{2}:[0-9]{2}:[0-9]{2}  "
                                      r"virgl_render_server\[[0-9]+\] <Debug>: socket disconnected"))


def _tail_end(raw: bytes) -> int | None:
    end = len(raw)
    for _ in range(MAX_AUDIO_TAIL_LINES + 1):
        if raw.endswith(FOOTER, 0, end):
            return end - len(FOOTER) if len(raw) - end <= MAX_AUDIO_TAIL_BYTES else None
        if not raw.endswith(b"\n", 0, end):
            return None
        start = raw.rfind(b"\n", 0, end - 1) + 1
        if not any(line.fullmatch(raw[start:end - 1].decode("ascii", "replace")) for line in HOST_TAIL):
            return None
        end = start
    return None


def terminal_stop(raw: bytes) -> tuple[int, str] | None:
    """Offset and text of the final report's stop record, or None."""
    tail_end = _tail_end(raw) if len(raw) < LOG_LIMIT else None
    if tail_end is None:
        return None
    counts, marker = [], raw.find(SERIAL, 0, tail_end)
    while marker >= 0:
        start = raw.rfind(b"\n", 0, marker) + 1
        count = COUNT_NEW.fullmatch(raw[start:marker])
        if count and int(count[1]) <= int(count[2]) == tail_end - marker - len(SERIAL):
            counts.append(start)
        marker = raw.find(SERIAL, marker + 1, tail_end)
    banner = raw.rfind(BANNER, 0, counts[0]) if len(counts) == 1 else -1
    stop = banner + len(BANNER)
    end = raw.find(b"\n", stop, counts[0]) if banner >= 0 else -1
    if end < 0 or not raw.startswith(b"stop: ", stop):
        return None
    try:
        return stop, raw[stop:end].decode("utf-8")
    except UnicodeDecodeError:
        return None


def system_off_offset(raw: bytes) -> int | None:
    """Offset of the final report's stop record when it is SYSTEM_OFF, else None."""
    stop = terminal_stop(raw)
    return stop[0] if stop is not None and stop[1] == SYSTEM_OFF else None
