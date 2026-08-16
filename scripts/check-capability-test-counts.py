#!/usr/bin/env python3
"""Count the tests a capability claim says it has.

A criterion that says "N tests cover X" in a named module is checkable, and two
were wrong: A12 claimed 17 where smccc_trng has 14, A13 claimed 19 where psci
has 13. Neither was caught, because the existing evidence gate only reads files
listed in evidence_paths and these criteria list none.

Counting uses `cargo test -- --list`, so a renamed or deleted test changes the
answer the same way a reader counting by hand would.
"""

from __future__ import annotations

import json
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
REGISTRY = os.path.join(ROOT, "capabilities", "windows-hvf.json")
# "... bridgevm_hvf::psci with 19 tests covering ..." and the reverse order.
CLAIM = re.compile(r"bridgevm_hvf::(\w+)\b")
COUNT = re.compile(r"(\d+) tests")


def listed_tests() -> list[str]:
    out = subprocess.run(
        ["cargo", "+1.97.0", "test", "-p", "bridgevm-hvf", "--lib", "--locked",
         "--", "--list"],
        cwd=ROOT, capture_output=True, text=True,
    )
    if out.returncode != 0:
        print("capability test counts: cargo test --list failed", file=sys.stderr)
        print(out.stderr[-500:], file=sys.stderr)
        sys.exit(1)
    return [
        line[: -len(": test")]
        for line in out.stdout.splitlines()
        if line.endswith(": test")
    ]


def main() -> int:
    registry = json.load(open(REGISTRY))
    tests = listed_tests()

    failures: list[str] = []
    checked = 0
    for criterion in registry["criteria"]:
        measured = criterion.get("measured", "")
        module = CLAIM.search(measured)
        count = COUNT.search(measured)
        if not module or not count:
            continue
        checked += 1
        name, claimed = module.group(1), int(count.group(1))
        actual = sum(1 for t in tests if t.startswith(f"{name}::"))
        if actual != claimed:
            failures.append(
                f"{criterion['id']}: claims {claimed} tests in "
                f"bridgevm_hvf::{name}, found {actual}"
            )

    if failures:
        for failure in failures:
            print(f"capability test counts: {failure}", file=sys.stderr)
        print(f"capability test counts: FAIL ({len(failures)})", file=sys.stderr)
        return 1

    print(f"capability test counts: PASS ({checked} claims counted)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
