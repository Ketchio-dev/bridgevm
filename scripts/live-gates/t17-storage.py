#!/usr/bin/env python3
"""Allocate and clean only a sealed T17 root; never fall back to internal media."""
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from t17_external_storage import JOB, MIN_BYTES, check_storage, external_lane, storage_row


def main():
    command, *args = sys.argv[1:]
    if command == "space":
        tier, manifest, home = args
        storage = storage_row(manifest) if tier == "t17-windows-hvf-product-e2e" else None
        if storage:
            available = check_storage(storage, minimum=MIN_BYTES)
        else:
            stats = os.statvfs(home)
            available = stats.f_bavail * stats.f_frsize
        print(available // 1024**3)
        return
    verified, job, *rest = args
    if not re.fullmatch(JOB, job):
        raise ValueError("invalid job id")
    storage = json.loads(Path(verified).read_text()).get("storage")
    parent = Path(storage["root"]) if storage else Path("/tmp")
    if storage:
        check_storage(storage, minimum=MIN_BYTES if command == "allocate" else 0)
    if command == "allocate":
        work = subprocess.check_output(["/usr/bin/mktemp", "-d", str(parent / f"bridgevm-e2e-{job}.XXXXXX")], text=True).strip()
        stat = Path(work).stat()
        with Path(verified + ".work.json").open("x") as record:
            json.dump({"work": work, "device": stat.st_dev, "inode": stat.st_ino}, record)
        print(work)
    elif command == "cleanup":
        work = Path(rest[0])
        if work.parent != parent or not re.fullmatch(rf"bridgevm-e2e-{re.escape(job)}\.[A-Za-z0-9]{{6}}", work.name):
            raise ValueError("cleanup escapes its job root")
        if work.is_symlink() or not work.is_dir() or work.stat().st_dev != parent.stat().st_dev:
            raise ValueError("cleanup root replaced or crossed a volume")
        identity = json.loads(Path(verified + ".work.json").read_text())
        if identity != {"work": str(work), "device": work.stat().st_dev, "inode": work.stat().st_ino}:
            raise ValueError("cleanup root identity changed after allocation")
        for current, dirs, _ in os.walk(work, followlinks=False):
            if any((Path(current) / name).is_symlink() or (Path(current) / name).stat().st_dev != parent.stat().st_dev for name in dirs):
                raise ValueError("cleanup contains a symlink or nested mount")
        shutil.rmtree(work)
    else:
        raise ValueError("unknown storage operation")


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        sys.exit(f"T17 storage refused: {error}")
