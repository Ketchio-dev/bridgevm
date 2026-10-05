#!/usr/bin/env python3
"""Read one repository-approved firmware digest, rejecting malformed pins."""
from pathlib import Path
import re
import sys


def read_pin(path: Path) -> str:
    raw = path.read_bytes()
    if re.fullmatch(rb"[0-9a-f]{64}\n", raw) is None:
        raise ValueError("firmware pin must be 64 lowercase hex digits and one newline")
    return raw[:-1].decode("ascii")


if __name__ == "__main__":
    try:
        print(read_pin(Path(sys.argv[1])))
    except (IndexError, OSError, ValueError) as error:
        raise SystemExit(f"invalid firmware pin: {error}") from error
