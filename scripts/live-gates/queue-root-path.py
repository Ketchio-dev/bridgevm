#!/usr/bin/env python3
"""Existing queue aliases may name only an already-private owned directory."""
import os
from pathlib import Path
import stat
import sys


def root(value):
    if not value or value.endswith("/") or value.split("/")[-1] in (".", ".."):
        raise ValueError("queue root needs an unambiguous directory leaf")
    path = Path(value)
    if path.is_symlink():
        path = path.resolve(strict=True); info = path.lstat()
        if not stat.S_ISDIR(info.st_mode) or info.st_uid != os.geteuid() or stat.S_IMODE(info.st_mode) != 0o700:
            raise ValueError("queue alias target must already be private and owned")
    return str(path)


if __name__ == "__main__":
    try: print(root(sys.argv[1]))
    except (OSError, ValueError, RuntimeError): raise SystemExit("queue root admission refused")
