#!/usr/bin/env python3
"""Fail when a file containing #[test] contributes no test to the binary.

A test file has to be reachable from the crate root to run at all, and nothing
reports a file that is not: a test that does not exist cannot fail. Splitting a
suite is exactly when the declaration gets forgotten, and this repository splits
suites constantly to stay inside its structural budgets.

Textual analysis is not enough. Files reach the build in three different ways
here -- `mod`, `#[path]`, and `include!` -- and the probe example generates its
`mod` lines from a macro. This asks the compiler what it actually built, via
`cargo test -- --list`, and requires every file's test functions to appear
somewhere in it.

    python3 scripts/check-tests-are-reachable.py
"""

from __future__ import annotations

import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
TOOLCHAIN = "+1.97.0"
TEST_FN = re.compile(r"#\[test\]\s*\n\s*(?:async\s+)?fn\s+(\w+)", re.M)

# Each entry is the cargo invocation that should contain the tests in a subtree.
SUITES = [
    (["test", "--workspace", "--locked"], ["crates", "runners", "tools"]),
    (
        ["test", "-p", "bridgevm-hvf", "--features", "venus", "--example",
         "hvf_gic_boot_probe", "--locked"],
        ["crates/bridgevm-hvf/examples"],
    ),
    # venus- and loom-gated tests exist only with their feature enabled; the
    # venus and loom gates run them separately.
    (["test", "-p", "bridgevm-hvf", "--features", "venus", "--lib", "--locked"], []),
    (["test", "-p", "bridgevm-hvf", "--example", "hvf_vtimer_cancel_probe", "--locked"], []),
]

# The loom models need RUSTFLAGS="--cfg loom" and a release build, which would
# make this gate minutes slower. check-loom.sh already fails when they compile
# to nothing, which is exactly the condition this gate looks for.
EXEMPT = {
    "crates/bridgevm-hvf/tests/loom_psci.rs",
    "crates/bridgevm-hvf/tests/loom_reset_generation.rs",
}


def listed_tests(args: list[str]) -> set[str]:
    result = subprocess.run(
        ["cargo", TOOLCHAIN, *args, "--", "--list"],
        cwd=ROOT, capture_output=True, text=True,
    )
    if result.returncode != 0:
        print(result.stderr[-2000:], file=sys.stderr)
        raise SystemExit(f"cargo {' '.join(args)} -- --list failed")
    return {
        line.rsplit(": ", 1)[0].rsplit("::", 1)[-1]
        for line in result.stdout.splitlines()
        if line.endswith(": test")
    }


def main() -> int:
    known: set[str] = set()
    for args, _ in SUITES:
        known |= listed_tests(args)

    tracked = subprocess.run(
        ["git", "ls-files", "*.rs"], cwd=ROOT, capture_output=True, text=True
    ).stdout.split()

    unreachable = []
    checked = 0
    for relative in tracked:
        if relative in EXEMPT:
            continue
        path = ROOT / relative
        names = set(TEST_FN.findall(path.read_text(errors="ignore")))
        if not names:
            continue
        checked += 1
        if not names & known:
            unreachable.append((relative, len(names)))

    if unreachable:
        for relative, count in unreachable:
            print(f"{relative}: {count} test(s) never compiled into any binary", file=sys.stderr)
        print(f"tests are reachable: FAIL ({len(unreachable)} file(s))", file=sys.stderr)
        return 1

    print(f"tests are reachable: PASS ({checked} files, {len(known)} tests)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
