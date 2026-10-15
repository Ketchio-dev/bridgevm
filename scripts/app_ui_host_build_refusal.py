#!/usr/bin/env python3
"""Recognize the exact release-only #error in native Swift and swiftbuild logs."""
from pathlib import Path
import sys

REFUSAL = b"BRIDGEVM_APP_UI_HOST requires a separate DEBUG diagnostic build"
GUARD = [b"#if BRIDGEVM_APP_UI_HOST && !DEBUG", b'#error("' + REFUSAL + b'")', b"#endif"]


def matches_refusal(log: bytes, source_path: str, status: str, source: bytes) -> bool:
    if status not in {str(value) for value in range(1, 256)}:
        return False
    if (not source_path.startswith("/") or Path(source_path).name != "BridgeVMControlMain.swift"
            or any(ord(character) < 32 or ord(character) == 127 for character in source_path)
            or source.split(b"\n")[3:6] != GUARD):
        return False
    location = source_path.encode("utf-8") + b":5:8"
    accepted = {location + b": error: " + REFUSAL, b"error: " + location + b" " + REFUSAL}
    return any(line in accepted for line in log.split(b"\n"))


def main() -> int:
    if len(sys.argv) != 4:
        print("usage: app_ui_host_build_refusal.py LOG ABSOLUTE_SOURCE NONZERO_COMPILER_STATUS", file=sys.stderr)
        return 2
    log, source, status = sys.argv[1:]
    try:
        return 0 if matches_refusal(Path(log).read_bytes(), source, status, Path(source).read_bytes()) else 1
    except (OSError, UnicodeError):
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
