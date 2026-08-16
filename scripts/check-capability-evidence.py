#!/usr/bin/env python3
"""Require a criterion's cited evidence to actually contain its numbers.

`measured` carries specific figures -- frame counts, hashes, run ids -- and
`evidence_paths` says where to check them. Nothing verified the two agree, and
for several criteria they do not, so a reader following the citation finds a
document that never mentions the number.

The generated capability matrix is excluded: it is rendered from this registry,
so finding a figure there proves only that the registry says it. Known gaps sit
in ACCEPTED, so a new unbacked claim fails without pretending the old ones are
fixed.
"""

from __future__ import annotations

import json
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
REGISTRY = ROOT / "capabilities/windows-hvf.json"

# Long numbers and hash-like ids. A trailing \b would drop every figure carrying
# a unit, so "1000ms" and "1280x1024" went unchecked; the lookahead keeps a hex
# id whole while allowing a unit after a decimal figure.
TOKEN = re.compile(r"\b[0-9]{4,}(?![0-9a-f])|\b[0-9a-f]{8,}\b")

# Criteria already uncited when this check was written (2026-08-16). Removing an
# entry is how a gap gets proven closed rather than assumed. B4 joined when the
# pattern started seeing units: its "through 1000ms" is in no cited document and
# re-measuring needs a live guest (scripts/measure-pointer-latency.sh).
ACCEPTED = {"A5", "A6", "A7", "A17", "A19", "B4"}


def main() -> int:
    registry = json.loads(REGISTRY.read_text())
    unbacked = []
    checked = 0

    for criterion in registry["criteria"]:
        tokens = set(TOKEN.findall(criterion.get("measured", "")))
        if not tokens:
            continue
        checked += 1
        for relative in criterion.get("evidence_paths", []):
            path = ROOT / relative
            if "capability-matrix" in relative or not path.is_file():
                continue
            if tokens & set(TOKEN.findall(path.read_text(errors="ignore"))):
                break
        else:
            unbacked.append(criterion["id"])

    new = [i for i in unbacked if i not in ACCEPTED]
    fixed = [i for i in ACCEPTED if i not in unbacked]

    for identifier in new:
        print(f"{identifier}: its measured figures appear in no cited evidence", file=sys.stderr)
    for identifier in sorted(fixed):
        print(f"{identifier}: now backed; remove it from ACCEPTED", file=sys.stderr)

    if new or fixed:
        print(f"capability evidence: FAIL ({len(new) + len(fixed)})", file=sys.stderr)
        return 1

    print(f"capability evidence: PASS ({checked} checked, {len(ACCEPTED)} known gaps)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
