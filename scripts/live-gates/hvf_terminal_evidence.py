#!/usr/bin/env python3
"""run.log evidence that only the final host report's framing can supply.

hvf_terminal_report binds the stop record and hvf_host_media the persistence
records right after it. host_tail returns the records after the report's footer:
host teardown records only (its HOST_TAIL), never guest copies.

    hvf_terminal_evidence.py --require-system-off|--require-nvme-write-back|--host-stats RUN_LOG

reads the whole RUN_LOG after the helper exited (a missing, linked, irregular or
oversized log reads as empty) and exits 0 only when it binds a SYSTEM_OFF stop,
exactly one NVMe disk write-back record, or a host-tail CoreAudio stats record,
which --host-stats prints; 1 otherwise.
"""

from __future__ import annotations

from pathlib import Path
import sys
from hvf_host_media import nvme_write_back_offset
from hvf_run_log import read_run_log
from hvf_terminal_report import FOOTER, LOG_LIMIT, system_off_offset, terminal_stop
REQUIRE = {"--require-system-off": system_off_offset, "--require-nvme-write-back": nvme_write_back_offset}


def host_tail(raw: bytes) -> list[str] | None:
    """The records after the bound final report's footer, or None when no report binds."""
    if terminal_stop(raw) is None:
        return None
    return raw[raw.rfind(FOOTER) + len(FOOTER):].decode("ascii").split("\n")[:-1]


def main(argv: list[str]) -> int:
    if len(argv) != 2 or argv[0] not in (*REQUIRE, "--host-stats"):
        print(f"usage: hvf_terminal_evidence.py {'|'.join(REQUIRE)}|--host-stats RUN_LOG", file=sys.stderr)
        return 2
    raw = read_run_log(Path(argv[1]), LOG_LIMIT - 1)
    if argv[0] in REQUIRE:
        return 0 if REQUIRE[argv[0]](raw) is not None else 1
    stats = [line for line in host_tail(raw) or () if line.startswith("hda CoreAudio stats:")]
    if stats:
        print(stats[-1])
    return 0 if stats else 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
