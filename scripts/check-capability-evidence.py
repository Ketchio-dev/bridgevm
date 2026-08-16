#!/usr/bin/env python3
"""Require a criterion's cited evidence to actually contain its numbers.

`measured` carries specific figures -- frame counts, hashes, run ids -- and
`evidence_paths` says where to check them. Nothing verified that the two agree,
and for five criteria they do not: the numbers appear in no cited file, so a
reader following the citation finds a document that never mentions them.

The generated capability matrix is excluded deliberately. It is rendered from
this registry, so finding a figure there proves only that the registry says it.

Existing gaps are listed in ACCEPTED below with the date they were recorded, so
this fails on a new unbacked claim without pretending the old ones are fixed.

    python3 scripts/check-capability-evidence.py
"""

from __future__ import annotations

import json
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
REGISTRY = ROOT / "capabilities/windows-hvf.json"

# Distinctive enough to be worth checking: long numbers and hash-like ids.
TOKEN = re.compile(r"\b[0-9]{4,}\b|\b[0-9a-f]{8,}\b")

# Criteria whose figures were already uncited when this check was written
# (2026-08-16). Each needs an evidence document that states them; removing an
# entry here is how that gets proven rather than assumed.
ACCEPTED = {"A5", "A6", "A7", "A17", "A19"}


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
