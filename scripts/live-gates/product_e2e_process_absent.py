#!/usr/bin/env python3
"""Only a successful bounded process observation can prove literal-path absence."""
import re
import os
import subprocess
import sys


def absent(path, run=subprocess.run):
    # pgrep uses an extended regex: quote every metacharacter in the path.
    pattern = re.sub(r"([.\[\]\\*^$()+?{}|])", r"\\\1", path)
    try:
        result = run(["/usr/bin/pgrep", "-f", pattern], stdin=subprocess.DEVNULL,
                     stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, timeout=10)
    except (OSError, subprocess.TimeoutExpired):
        return False
    if result.returncode == 1:
        return True
    if result.returncode != 0:
        return False
    try:
        pids = [int(pid) for pid in result.stdout.split()]
    except ValueError:
        return False
    # This observer's argv contains the path. Exclude only this exact process;
    # pgrep already excludes itself, and all other matching processes retain it.
    return bool(pids) and set(pids) == {os.getpid()}


if __name__ == "__main__":
    raise SystemExit(0 if len(sys.argv) == 2 and absent(sys.argv[1]) else 1)
