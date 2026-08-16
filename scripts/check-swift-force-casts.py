#!/usr/bin/env python3
"""Keep `try!` and `as!` out of the app sources.

Both crash the process rather than surfacing an error, and neither is needed
here. The instance that prompted this round-tripped a dictionary through
CFDictionary and forced it back: toll-free bridging made it safe in practice,
but nothing in the code said so, and the next such cast might not be.

    python3 scripts/check-swift-force-casts.py
"""

from __future__ import annotations

import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
SOURCES = ROOT / "apps/macos/Sources"
FORCE_CAST = re.compile(r"\btry!\s|\bas!\s")


def main() -> int:
    if not SOURCES.is_dir():
        print(f"swift force casts: FAIL (missing {SOURCES})", file=sys.stderr)
        return 1

    found = []
    scanned = 0
    for path in sorted(SOURCES.rglob("*.swift")):
        scanned += 1
        for number, line in enumerate(path.read_text(errors="ignore").splitlines(), 1):
            text = line.strip()
            if text.startswith("//"):
                continue
            if FORCE_CAST.search(text):
                found.append(f"{path.relative_to(ROOT)}:{number}  {text[:70]}")

    if found:
        for location in found:
            print(location, file=sys.stderr)
        print(f"swift force casts: FAIL ({len(found)})", file=sys.stderr)
        return 1

    if scanned == 0:
        print("swift force casts: FAIL (no Swift sources found)", file=sys.stderr)
        return 1

    print(f"swift force casts: PASS ({scanned} files)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
