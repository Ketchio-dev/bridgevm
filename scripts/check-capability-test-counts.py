#!/usr/bin/env python3
"""Count what a capability claim says it counted.

Three claims were wrong: A12 said 17 where smccc_trng has 14, A13 said 19 where
psci has 13, and A11 said 26 steps after the gate became 27. The evidence gate
reads only files in evidence_paths, and these criteria list none.

Counting uses `cargo test -- --list` and the step declarations, so a renamed
test or a new gate changes the answer as a reader counting would.
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
# "... runs 26 steps ...": adding a gate changes this, and it is copied by hand.
STEPS = re.compile(r"runs (\d+) steps")


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
    return [l[: -len(": test")] for l in out.stdout.splitlines() if l.endswith(": test")]


def main() -> int:
    registry = json.load(open(REGISTRY))
    tests = listed_tests()

    with open(os.path.join(ROOT, "scripts", "check-project.sh")) as handle:
        steps = sum(1 for line in handle if line.strip().startswith("step "))

    failures: list[str] = []
    checked = 0
    for criterion in registry["criteria"]:
        measured = criterion.get("measured", "")
        claimed = STEPS.search(measured)
        if claimed:
            checked += 1
            if int(claimed.group(1)) != steps:
                failures.append(f"{criterion['id']}: claims check-project.sh "
                                f"runs {claimed.group(1)} steps, found {steps}")
        module = CLAIM.search(measured)
        count = COUNT.search(measured)
        if not module or not count:
            continue
        checked += 1
        name, claimed = module.group(1), int(count.group(1))
        actual = sum(1 for t in tests if t.startswith(f"{name}::"))
        if actual != claimed:
            failures.append(f"{criterion['id']}: claims {claimed} tests in "
                            f"bridgevm_hvf::{name}, found {actual}")

    if failures:
        for failure in failures:
            print(f"capability test counts: {failure}", file=sys.stderr)
        print(f"capability test counts: FAIL ({len(failures)})", file=sys.stderr)
        return 1

    print(f"capability test counts: PASS ({checked} claims counted)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
