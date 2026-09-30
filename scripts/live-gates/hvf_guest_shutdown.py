"""A clean HVF guest shutdown: exit status 0 and hvf_stop_line.SYSTEM_OFF as the final report's stop.

hvf_terminal_report binds that record to the host's report framing; run_log is read after the helper exited.
"""

from __future__ import annotations

from pathlib import Path
from hvf_terminal_report import system_off_offset


def guest_shutdown_observed(exit_code: object, run_log: Path) -> bool:
    raw = run_log.read_bytes() if run_log.is_file() and run_log.stat().st_size <= 64 * 1024 * 1024 else b""
    return type(exit_code) is int and exit_code == 0 and system_off_offset(raw) is not None
