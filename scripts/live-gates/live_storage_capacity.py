#!/usr/bin/env python3
"""Pre-start filesystem reserve, not workload sizing or sparse-backing admission."""
import argparse
import os
from pathlib import Path
import re
import stat
import sys

GIB = 1024 ** 3


def threshold(value):
    if not isinstance(value, str) or not re.fullmatch(r"0|[1-9][0-9]{0,3}", value) or int(value) > 4096:
        raise ValueError("minimum free GiB must be a canonical integer in 0..4096")
    return int(value) * GIB


def storage_path(value):
    if not value or not value.startswith("/") or value.endswith("/") or any(c in value for c in "\x00\r\n"):
        raise ValueError("storage path must be absolute with an unambiguous leaf")
    if any(part in (".", "..") for part in value.split("/")):
        raise ValueError("ambiguous storage path")
    path = Path(value)
    if path.is_symlink(): raise ValueError("storage root leaf is an alias")
    canonical = path.resolve()
    if canonical.parts[:2] == ("/", "Volumes") and len(canonical.parts) >= 3:
        if not os.path.ismount(Path(*canonical.parts[:3])):
            raise ValueError("selected volume is not mounted")
    return path


def available(paths, *, estimate=False):
    devices = {}
    for value in paths:
        path = storage_path(value)
        if estimate:
            while not os.path.lexists(path):
                path = path.parent
        canonical = path.resolve(strict=True)
        fd = os.open(canonical, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
        try:
            info = os.fstat(fd)
            if not stat.S_ISDIR(info.st_mode): raise ValueError("storage is not a directory")
            if info.st_dev in devices: continue
            data = os.fstatvfs(fd)
            if (type(data.f_frsize) is not int or data.f_frsize <= 0
                    or type(data.f_bavail) is not int or data.f_bavail < 0
                    or data.f_bavail > data.f_blocks or data.f_flag & os.ST_RDONLY):
                raise ValueError("invalid or read-only filesystem capacity")
            devices[info.st_dev] = data.f_bavail * data.f_frsize
        finally:
            os.close(fd)
    if not devices: raise ValueError("no storage paths")
    return min(devices.values())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--estimate", action="store_true")
    mode.add_argument("--paths-only", action="store_true")
    mode.add_argument("--prepare-work", action="store_true")
    parser.add_argument("--minimum", required=True)
    parser.add_argument("paths", nargs="+")
    args = parser.parse_args()
    try:
        required = threshold(args.minimum)
        paths = [storage_path(path) for path in args.paths]
        if args.paths_only: return 0
        if args.prepare_work:
            if len(paths) != 2: raise ValueError("expected job and work paths")
            paths[0].resolve(strict=True)
            paths[1].mkdir(mode=0o700, exist_ok=True)
        free = available(args.paths, estimate=args.estimate)
    except (OSError, ValueError, RuntimeError):
        print("storage capacity unavailable or configuration invalid", file=sys.stderr)
        return 2
    print(free // GIB)
    return 0 if free >= required else 3


if __name__ == "__main__": sys.exit(main())
