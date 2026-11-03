#!/usr/bin/env python3
"""Only a successful bounded process observation can prove literal-path absence.

`--end PATH` ends the processes naming PATH: TERM, then KILL what remains. A
canceled T17/T19 tier uses it for its own lane tree, because the helper is
launched through LaunchServices and so is outside the tier's process group (r97).
"""
import os
import re
import signal
import subprocess
import sys
import time


def matching(path, run=subprocess.run):
    """Other processes whose command line contains PATH, or None when unobservable."""
    # pgrep uses an extended regex: quote every metacharacter in the path.
    pattern = re.sub(r"([.\[\]\\*^$()+?{}|])", r"\\\1", path)
    try:
        result = run(["/usr/bin/pgrep", "-f", pattern], stdin=subprocess.DEVNULL,
                     stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, timeout=10)
    except (OSError, subprocess.TimeoutExpired):
        return None
    if result.returncode == 1:
        return set()
    if result.returncode != 0:
        return None
    try:
        pids = {int(pid) for pid in result.stdout.split()}
    except ValueError:
        return None
    # This observer's argv contains the path. Exclude only this exact process;
    # pgrep already excludes itself, and all other matching processes retain it.
    return None if not pids else pids - {os.getpid()}


def absent(path, run=subprocess.run):
    return matching(path, run) == set()


def end(path, run=subprocess.run, term_seconds=20.0, kill_seconds=5.0):
    """TERM every process naming PATH, KILL survivors; True once none remains."""
    for sig, seconds in ((signal.SIGTERM, term_seconds), (signal.SIGKILL, kill_seconds)):
        deadline = time.monotonic() + seconds
        while True:
            pids = matching(path, run)
            if pids is None:
                return False
            if not pids:
                return True
            if time.monotonic() >= deadline:
                break
            for pid in pids:
                try:
                    os.kill(pid, sig)
                except OSError:
                    pass  # already gone, or not ours: the next observation decides
            time.sleep(0.2)
    return False


if __name__ == "__main__":
    if len(sys.argv) == 3 and sys.argv[1] == "--end":
        raise SystemExit(0 if end(sys.argv[2]) else 1)
    raise SystemExit(0 if len(sys.argv) == 2 and absent(sys.argv[1]) else 1)
