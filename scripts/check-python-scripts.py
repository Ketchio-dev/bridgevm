#!/usr/bin/env python3
"""Fail when a tracked Python script does not compile.

Several gates in this repository are Python, and a syntax error in one is
invisible to every other check: it surfaces only when something runs it, which
for a gate can be a hosted CI run days later. Compiling them costs a moment.

    python3 scripts/check-python-scripts.py
"""

from __future__ import annotations

import py_compile
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def main() -> int:
    tracked = subprocess.run(
        ["git", "ls-files", "*.py"], cwd=ROOT, capture_output=True, text=True
    ).stdout.split()
    if not tracked:
        print("python scripts: FAIL (no Python scripts found)", file=sys.stderr)
        return 1

    failures = []
    with tempfile.TemporaryDirectory() as cache:
        for relative in tracked:
            target = Path(cache) / (relative.replace("/", "_") + "c")
            try:
                py_compile.compile(
                    str(ROOT / relative), cfile=str(target), doraise=True
                )
            except py_compile.PyCompileError as error:
                failures.append(f"{relative}: {error.msg.strip().splitlines()[-1]}")

    if failures:
        for failure in failures:
            print(failure, file=sys.stderr)
        print(f"python scripts: FAIL ({len(failures)})", file=sys.stderr)
        return 1

    print(f"python scripts: PASS ({len(tracked)} scripts)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
