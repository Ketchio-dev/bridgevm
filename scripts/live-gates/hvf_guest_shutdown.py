"""A clean HVF guest shutdown: exit status 0 and hvf_stop_line.SYSTEM_OFF as the final report's stop.

hvf_terminal_report binds it to host framing in run_log, read once the helper exited: regular, unlinked, <= 64 MiB.
"""

from __future__ import annotations

from pathlib import Path
from hvf_run_log import read_run_log
from hvf_terminal_report import system_off_offset


def guest_shutdown_observed(exit_code: object, run_log: Path) -> bool:
    return type(exit_code) is int and exit_code == 0 and system_off_offset(read_run_log(run_log, 64 << 20)) is not None
