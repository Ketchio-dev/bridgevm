#!/usr/bin/env python3
"""Keep force casts and machine-specific paths out of the app sources.

A force cast crashes instead of surfacing an error. A hardcoded home path is
meaningless elsewhere; one was autofilling the installer ISO field.

    python3 scripts/check-swift-force-casts.py
"""

from __future__ import annotations

import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
SOURCES = ROOT / "apps/macos/Sources"
FORCE_CAST = re.compile(r"\btry!\s|\bas!\s")

MACHINE_PATH = re.compile(r'"/Users/(?!me/)[^"]*"')  # NSHomeDirectory() instead


def main() -> int:
    if not SOURCES.is_dir():
        print(f"swift source hygiene: FAIL (missing {SOURCES})", file=sys.stderr)
        return 1

    found = []
    scanned = 0
    for path in sorted(SOURCES.rglob("*.swift")):
        scanned += 1
        for number, line in enumerate(path.read_text(errors="ignore").splitlines(), 1):
            text = line.strip()
            if text.startswith("//"):
                continue
            if FORCE_CAST.search(text) or MACHINE_PATH.search(text):
                found.append(f"{path.relative_to(ROOT)}:{number}  {text[:70]}")

    if found:
        for location in found:
            print(location, file=sys.stderr)
        print(f"swift source hygiene: FAIL ({len(found)})", file=sys.stderr)
        return 1

    if scanned == 0:
        print("swift source hygiene: FAIL (no Swift sources found)", file=sys.stderr)
        return 1

    print(f"swift source hygiene: PASS ({scanned} files)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
