"""The whole run.log a reader binds, read once the helper exited.

One descriptor opened with O_NOFOLLOW is checked with fstat and then read, so a
link cannot replace the file between the check and the read. O_NONBLOCK keeps a
FIFO from stalling the open; it is then refused as irregular.
"""

from __future__ import annotations

import os
from pathlib import Path
import stat


def read_run_log(path: Path, limit: int) -> bytes:
    """The whole regular, unlinked file at path of at most limit bytes, else b""."""
    try:
        with open(os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK), "rb") as stream:
            raw = stream.read(limit + 1) if stat.S_ISREG(os.fstat(stream.fileno()).st_mode) else b""
    except OSError:
        return b""
    return raw if len(raw) <= limit else b""
