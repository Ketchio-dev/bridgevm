#!/usr/bin/env python3
"""Refuse removal of a T17 job tree while host state may still reference it.

Every probe fails closed: a mount table or disk-image list that cannot be read
within the harvest's release bound counts as residue, and so does a guest-setup
harvest whose own release was not proven. The tier then reports cleanup-failed
and keeps the tree for the operator.
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
import product_e2e_work as BOUNDARY
import t17_guest_setup_harvest as HARVEST  # noqa: E402
from product_e2e_host_listing import listing
from t17_private_diagnostic_packet import INDEX_CAP  # noqa: E402

MOUNT = "/sbin/mount"
HDIUTIL = HARVEST.HDIUTIL
WORK = re.compile(r"bridgevm-e2e-(" + BOUNDARY.JOB + r")\.[A-Za-z0-9]{6}\Z")
LANE = re.compile(r"lane-[1-3]\Z")
INDEX = re.compile(r"t17-diagnostic-lane-[1-3]-index\.json\Z")
READ_FLAGS = os.O_RDONLY | os.O_NOFOLLOW | os.O_CLOEXEC | os.O_NONBLOCK
DIR_FLAGS = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC


def recorded_cleanup(private: int, name: str) -> object:
    """The guest_setup.cleanup value a private packet index records, if readable."""
    fd = os.open(name, READ_FLAGS, dir_fd=private)
    try:
        info = os.fstat(fd)
        if not stat.S_ISREG(info.st_mode) or info.st_size > INDEX_CAP:
            return None
        value = json.loads(os.read(fd, INDEX_CAP + 1))
    finally:
        os.close(fd)
    setup = value.get("guest_setup") if isinstance(value, dict) else None
    return setup.get("cleanup") if isinstance(setup, dict) else None


def residue(work: str, private: str) -> str | None:
    """Return why host state may still reference the tree, or None when nothing does."""
    match = WORK.fullmatch(Path(work).name)
    if not match or Path(work).parent != Path(private):
        return "work-outside-job-boundary"
    BOUNDARY.directory(private); BOUNDARY.directory(work)
    table = listing(MOUNT)
    if table is None or b" on / (" not in table:
        return "mount-table-unreadable"
    if work.encode() in table:
        return "mounted"
    images = listing(HDIUTIL, "info", "-plist")
    if images is None or not isinstance(HARVEST.plist(images).get("images"), list):
        return "image-list-unreadable"
    if work.encode() in images:
        return "attached"
    for name in sorted(os.listdir(work)):
        if LANE.fullmatch(name) and os.path.lexists(os.path.join(work, name, HARVEST.WORK_NAME)):
            return "guest-setup-harvest-unreleased"
    directory = os.open(private, DIR_FLAGS)
    try:
        for name in sorted(os.listdir(directory)):
            if not INDEX.fullmatch(name):
                continue
            try:
                cleanup = recorded_cleanup(directory, name)
            except (OSError, ValueError):
                cleanup = None
            if cleanup not in ("not-needed", "verified"):
                return "guest-setup-release-unproven"
    finally:
        os.close(directory)
    return None


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--work", required=True)
    parser.add_argument("--private", required=True)
    args = parser.parse_args()
    try:
        reason = residue(args.work, args.private)
    except OSError as error:
        reason = f"unreadable-{type(error).__name__}"
    if reason is None:
        return 0
    print(f"T17 cleanup refused: host residue {reason}", file=sys.stderr)
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
