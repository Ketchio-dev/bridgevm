#!/usr/bin/env python3
"""run.log evidence that only the final host report's framing can supply.

hvf_terminal_report binds the stop record. The records after that report's
footer are host teardown records only (its HOST_TAIL), and no later footer can
occur in them, so they are the host's CoreAudio lifecycle, callback and stats
lines. Guest copies before the report or inside its tail are not among them.

    hvf_terminal_evidence.py --require-system-off RUN_LOG

exits 0 only when the whole RUN_LOG, read after the helper exited, binds a
SYSTEM_OFF stop; 1 otherwise, including for a missing, linked or oversized log.
"""

from __future__ import annotations

from pathlib import Path
import sys
from hvf_terminal_report import FOOTER, LOG_LIMIT, system_off_offset, terminal_stop


def read_run_log(path: Path) -> bytes:
    """The whole regular, unlinked run.log below LOG_LIMIT bytes, else b""."""
    try:
        if path.is_symlink() or not path.is_file() or path.stat().st_size >= LOG_LIMIT:
            return b""
        return path.read_bytes()
    except OSError:
        return b""


def host_tail(raw: bytes) -> list[str] | None:
    """The records after the bound final report's footer, or None when no report binds."""
    if terminal_stop(raw) is None:
        return None
    return raw[raw.rfind(FOOTER) + len(FOOTER):].decode("ascii").split("\n")[:-1]


def main(argv: list[str]) -> int:
    if len(argv) != 2 or argv[0] != "--require-system-off":
        print("usage: hvf_terminal_evidence.py --require-system-off RUN_LOG", file=sys.stderr)
        return 2
    return 0 if system_off_offset(read_run_log(Path(argv[1]))) is not None else 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
