#!/usr/bin/env python3
"""run.log evidence that only the final host report's framing can supply.

hvf_terminal_report binds the stop record. The records after that report's
footer are host teardown records only (its HOST_TAIL), and no later footer can
occur in them, so they are the host's CoreAudio lifecycle, callback and stats
lines. Guest copies before the report or inside its tail are not among them.

    hvf_terminal_evidence.py --require-system-off RUN_LOG
    hvf_terminal_evidence.py --host-stats RUN_LOG

read the whole RUN_LOG after the helper exited; a missing, linked, irregular or
oversized log reads as empty. The first exits 0 only when RUN_LOG binds a
SYSTEM_OFF stop. The second prints the host tail's last CoreAudio stats record
and exits 0 only when there is one. Both exit 1 otherwise.
"""

from __future__ import annotations

from pathlib import Path
import sys
from hvf_run_log import read_run_log
from hvf_terminal_report import FOOTER, LOG_LIMIT, system_off_offset, terminal_stop


def host_tail(raw: bytes) -> list[str] | None:
    """The records after the bound final report's footer, or None when no report binds."""
    if terminal_stop(raw) is None:
        return None
    return raw[raw.rfind(FOOTER) + len(FOOTER):].decode("ascii").split("\n")[:-1]


def main(argv: list[str]) -> int:
    if len(argv) != 2 or argv[0] not in ("--require-system-off", "--host-stats"):
        print("usage: hvf_terminal_evidence.py --require-system-off|--host-stats RUN_LOG", file=sys.stderr)
        return 2
    raw = read_run_log(Path(argv[1]), LOG_LIMIT - 1)
    if argv[0] == "--require-system-off":
        return 0 if system_off_offset(raw) is not None else 1
    stats = [line for line in host_tail(raw) or () if line.startswith("hda CoreAudio stats:")]
    if stats:
        print(stats[-1])
    return 0 if stats else 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
