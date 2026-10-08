#!/usr/bin/env python3
"""Stable same-volume T19 source destination for a queued T17 job."""
import os
from pathlib import Path
import re
import stat
import sys


def directory(path, private=True):
    info = path.lstat()
    if (path.resolve() != path or not stat.S_ISDIR(info.st_mode)
            or info.st_uid != os.geteuid() or info.st_mode & 0o022
            or (private and stat.S_IMODE(info.st_mode) != 0o700)):
        raise ValueError("queued retention directory must be canonical, private and owned")
    return info.st_dev


def destination(output, job):
    if type(job) is not str or not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]{0,127}", job):
        raise ValueError("invalid queued retention job")
    out = Path(output).resolve(strict=True)
    if out.name != job or out.parent.name != "running":
        raise ValueError("retention requires the exact running job layout")
    queue = out.parent.parent
    device = directory(out)
    if directory(out.parent, private=False) != device or directory(queue) != device:
        raise ValueError("queued retention crossed a device")
    parent = queue / "t19-sources"
    try:
        parent.mkdir(mode=0o700)
    except FileExistsError:
        pass
    if directory(parent) != device:
        raise ValueError("retained sources must stay on the queue volume")
    target = parent / job
    if os.path.lexists(target):
        raise ValueError("retained source already exists")
    return target


if __name__ == "__main__":
    try:
        if len(sys.argv) != 3: raise ValueError("expected output and job")
        print(destination(sys.argv[1], sys.argv[2]))
    except (OSError, ValueError):
        raise SystemExit("queued retained-source placement refused")
