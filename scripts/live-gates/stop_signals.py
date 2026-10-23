"""Let a live-gate tool's finally blocks run when SIGTERM, SIGINT or SIGHUP stops it.

Python's default SIGTERM and SIGHUP actions end the process at once, so cleanup
in a finally block never runs, and a queue cancel sends SIGTERM. While a tool
runs through stoppable(), each of the three signals instead raises SystemExit
with 128 plus its number, as a shell reports a signal. The first one also sets
all three to be ignored, so a repeat cannot interrupt the cleanup it started.
SIGKILL cannot be caught.
"""
from __future__ import annotations

import signal
from typing import Callable

STOP_SIGNALS = (signal.SIGTERM, signal.SIGINT, signal.SIGHUP)


def stop(signum: int, _frame: object) -> None:
    for each in STOP_SIGNALS:
        signal.signal(each, signal.SIG_IGN)
    raise SystemExit(128 + signum)


def stoppable(function: Callable[[], int]) -> int:
    """Install the stop handlers, then return FUNCTION's exit status."""
    for signum in STOP_SIGNALS:
        signal.signal(signum, stop)
    return function()
