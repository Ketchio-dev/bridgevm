"""Read a B9 immutable queue ledger from one bounded regular descriptor."""

from __future__ import annotations

import os
from pathlib import Path
import stat

from b9_share_asset_integrity import parent_chain


def _identity(item) -> tuple[int, int, int, int, int, int]:
    return (item.st_dev, item.st_ino, item.st_size, item.st_mtime_ns,
            item.st_ctime_ns, item.st_mode)


def read_immutable_ledger(path: Path, limit: int) -> bytes:
    parent_chain(path)
    before = os.lstat(path)
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    with os.fdopen(fd, "rb") as stream:
        opened = os.fstat(stream.fileno())
        if (not stat.S_ISREG(opened.st_mode) or opened.st_nlink != 1
                or not 0 < opened.st_size <= limit or opened.st_mode & 0o222
                or _identity(before) != _identity(opened)):
            raise ValueError("B9 immutable ledger type, mode or size differs")
        raw = stream.read(limit + 1)
        after = os.fstat(stream.fileno())
    if (len(raw) != opened.st_size or _identity(opened) != _identity(after)
            or _identity(opened) != _identity(os.lstat(path))):
        raise ValueError("B9 immutable ledger changed while reading")
    return raw
