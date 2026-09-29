"""A clean HVF guest shutdown: exit status 0 and the exact SYSTEM_OFF record.

run_log_lines are run.log records already split on line breaks. A record counts
only by equality, so a line that merely contains the stop text does not.
"""

from __future__ import annotations

from hvf_stop_line import SYSTEM_OFF


def guest_shutdown_observed(exit_code: object, run_log_lines: list[str]) -> bool:
    return (type(exit_code) is int and exit_code == 0
            and any(line == SYSTEM_OFF for line in run_log_lines))
