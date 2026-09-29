#!/usr/bin/env python3
"""Refuse app-UI tiers unless this user's console session is on screen and unlocked."""
from __future__ import annotations

import argparse
import os
import plistlib
import subprocess
import sys

IOREG = "/usr/sbin/ioreg"


class SessionError(ValueError):
    pass


def console_state(ioreg_plist: bytes, uid: int) -> str:
    """Return 'unlocked' for exactly one on-console session of uid, else raise.

    WindowServer publishes CGSSessionScreenIsLocked only while the session is
    locked, so its absence on an on-console, logged-in session means unlocked.
    """
    try:
        root = plistlib.loads(ioreg_plist)
    except Exception as error:
        raise SessionError(f"console session state is unreadable: {error}") from error
    node = root[0] if isinstance(root, list) and len(root) == 1 else root
    sessions = node.get("IOConsoleUsers") if isinstance(node, dict) else None
    if not isinstance(sessions, list):
        raise SessionError("console session state has no IOConsoleUsers list")
    mine = [s for s in sessions if isinstance(s, dict) and s.get("kCGSSessionUserIDKey") == uid]
    if len(mine) != 1:
        raise SessionError(f"expected one console session for uid {uid}, found {len(mine)}")
    session = mine[0]
    if session.get("kCGSSessionOnConsoleKey") is not True:
        raise SessionError("this user's session is not on the console (switched out or remote)")
    if session.get("kCGSessionLoginDoneKey") is not True:
        raise SessionError("this user's console login has not finished")
    locked = session.get("CGSSessionScreenIsLocked", False)
    if locked is not False:
        raise SessionError("the console screen is locked; app-UI tiers need an unlocked session")
    return "unlocked"


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--require-unlocked", action="store_true", required=True)
    parser.parse_args(argv)
    try:
        result = subprocess.run([IOREG, "-n", "Root", "-d1", "-a"], capture_output=True, timeout=20, check=False)
        if result.returncode != 0:
            raise SessionError(f"ioreg exited {result.returncode}")
        console_state(result.stdout, os.getuid())
    except (SessionError, OSError, subprocess.TimeoutExpired) as error:
        print(f"refusing app-UI tier before a job id is used: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
