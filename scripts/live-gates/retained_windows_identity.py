"""Identify retention directories without following a replaced name."""
from __future__ import annotations

from pathlib import Path
import stat


def directory_identity(path: Path) -> tuple[int, int] | None:
    try:
        metadata = path.lstat()
    except FileNotFoundError:
        return None
    if not stat.S_ISDIR(metadata.st_mode):
        return None
    return metadata.st_dev, metadata.st_ino
