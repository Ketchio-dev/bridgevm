"""Recognize an authenticated T17 terminal report with bounded host audio tail."""
from __future__ import annotations

import re

ACK = re.compile(rb"^HOST-DIAGNOSTIC-STOP: generation=([0-9]+) nonce=([0-9a-f]{32}) request consumed; ending run through final report$", re.M)
COMPLETE_DETAIL = re.compile(r"host_stop=status=complete,generation=([0-9]+),nonce=([0-9a-f]{32}),report=complete,helper=terminal,log_offset=([0-9]+)(?:;|\Z)")
MAX_AUDIO_TAIL_BYTES = 4 * 1024
MAX_AUDIO_TAIL_LINES = 16
FOOTER = b"\n--- end ---\n"
SERIAL = b"\n--- serial (tail) ---\n"
BANNER = b"\n=== EDK2 boot probe (with Apple hv_gic) ===\n"
STOP = b"\nstop: host diagnostic stop requested\n"
COUNT_NEW = re.compile(rb"(?:^|\n)serial raw bytes: ([0-9]{1,8}) output bytes: ([0-9]{1,8})\Z")
COUNT_LEGACY = re.compile(rb"(?:^|\n)serial bytes: ([0-9]{1,8})\Z")
AUDIO_LINES = (
    re.compile(r"hda CoreAudio callback enqueue: state=stopping reason=[a-z-]+ osstatus=-?[0-9]+ expected=(true|false)"),
    re.compile(r"hda CoreAudio lifecycle: operation=(stop|dispose) osstatus=-?[0-9]+ success=(true|false)"),
    re.compile(r"hda CoreAudio stats: [a-z][a-z0-9_]*=[0-9]+( [a-z][a-z0-9_]*=[0-9]+)*"),
)


def bounded_host_shutdown_tail(tail: str) -> bool:
    if not tail:
        return True
    if len(tail.encode("utf-8")) > MAX_AUDIO_TAIL_BYTES or not tail.endswith("\n"):
        return False
    lines = tail[:-1].split("\n")
    return 1 <= len(lines) <= MAX_AUDIO_TAIL_LINES and all(
        any(pattern.fullmatch(line) for pattern in AUDIO_LINES) for line in lines
    )


def terminal_report(tail: bytes, detail: str, tail_offset: int) -> tuple[int, str] | None:
    claim = COMPLETE_DETAIL.search(detail)
    if claim is None or int(claim[3]) < tail_offset or int(claim[3]) > tail_offset + len(tail):
        return None
    raw = tail[int(claim[3]) - tail_offset:]
    ack = next((match for match in ACK.finditer(raw) if match[2] == claim[2].encode()), None)
    if ack is None or ack[1] != claim[1].encode():
        return None
    serial = raw.find(SERIAL, ack.end())
    banner = raw.find(BANNER, ack.end(), serial)
    stop = raw.find(STOP, banner, serial)
    if serial < 0 or banner < 0 or stop < 0:
        return None
    header = raw[stop + len(STOP):serial]
    modern = COUNT_NEW.search(header)
    legacy = modern is None
    if modern is not None:
        raw_size, size = int(modern[1]), int(modern[2])
        if size < raw_size:
            return None
    else:
        old = COUNT_LEGACY.search(header)
        if old is None:
            return None
        size = int(old[1])
    footer = serial + len(SERIAL) + size
    if footer > len(raw) or raw[footer:footer + len(FOOTER)] != FOOTER:
        return None
    try:
        displayed = raw[serial + len(SERIAL):footer].decode("utf-8")
        suffix = raw[footer + len(FOOTER):].decode("ascii")
    except UnicodeDecodeError:
        return None
    if (legacy and "\ufffd" in displayed) or not bounded_host_shutdown_tail(suffix):
        return None
    return int(claim[1]), raw[stop:serial].decode("utf-8", errors="replace")
