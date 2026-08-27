#!/usr/bin/env python3
"""Compile tracked Python scripts and run deterministic gate self-tests."""
from __future__ import annotations
import py_compile
import subprocess
import sys
import tempfile
from pathlib import Path
ROOT = Path(__file__).resolve().parent.parent
SELF_TESTS = ("scripts/audio-playback-result.py",)

def main() -> int:
    tracked = subprocess.run(
        ["git", "ls-files", "*.py"],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=True,
    ).stdout.split()
    if not tracked:
        print("python scripts: FAIL (no Python scripts found)", file=sys.stderr)
        return 1
    failures: list[str] = []
    with tempfile.TemporaryDirectory() as cache:
        for relative in tracked:
            target = Path(cache) / (relative.replace("/", "_") + "c")
            try:
                py_compile.compile(str(ROOT / relative), cfile=str(target), doraise=True)
            except py_compile.PyCompileError as error:
                failures.append(f"{relative}: {error.msg.strip().splitlines()[-1]}")
    for relative in SELF_TESTS:
        result = subprocess.run(
            [sys.executable, str(ROOT / relative), "--self-test"],
            cwd=ROOT,
            capture_output=True,
            text=True,
        )
        output = (result.stderr or result.stdout).strip()
        if result.returncode:
            failures.append(f"{relative} --self-test: {output or 'failed silently'}")
        elif result.stdout.strip():
            print(result.stdout.strip())
    if failures:
        print(*failures, sep="\n", file=sys.stderr)
        print(f"python scripts: FAIL ({len(failures)})", file=sys.stderr)
        return 1
    print(f"python scripts: PASS ({len(tracked)} scripts, {len(SELF_TESTS)} self-tests)")
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
