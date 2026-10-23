"""Keep child output and OS error text out of a T19 handoff's tier log.

The tier log may carry only booleans, sizes, hashes and fixed tokens, and the
packaged app's messages, cp's errors and an OS error's text can all name private
paths. A child's stdout and stderr are captured with a bound and a timeout, as
t17_lane_key_forget.py does, and never echoed; a failure is a fixed refusal
naming only the exit status, the errno name or the error class.
"""
from __future__ import annotations

import errno
import subprocess

TIMEOUT_SECONDS = 120
OUTPUT_LIMIT = 65_536


def errno_name(error: OSError) -> str:
    return errno.errorcode.get(error.errno, "no errno")


def run_quietly(task: str, argv: list[str]) -> None:
    try:
        completed = subprocess.run(argv, stdin=subprocess.DEVNULL, capture_output=True,
                                   timeout=TIMEOUT_SECONDS, check=False)
    except subprocess.TimeoutExpired:
        raise ValueError(f"{task} did not finish within {TIMEOUT_SECONDS} s") from None
    except OSError as error:
        raise ValueError(f"{task} could not start ({errno_name(error)})") from None
    if len(completed.stdout) > OUTPUT_LIMIT or len(completed.stderr) > OUTPUT_LIMIT:
        raise ValueError(f"{task} output exceeds {OUTPUT_LIMIT} bytes")
    if completed.returncode != 0:
        raise ValueError(f"{task} failed with exit status {completed.returncode}")


def describe(error: BaseException) -> str:
    """A refusal's own text; an OS or helper-process error only by its errno name or class."""
    if isinstance(error, OSError):
        return f"operating-system error ({errno_name(error)})"
    if isinstance(error, subprocess.SubprocessError):
        return f"helper process error ({type(error).__name__})"
    return str(error)
