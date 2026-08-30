#!/usr/bin/env python3
"""Fail when a file containing #[test] contributes no test to the binary.

A test file must be reachable from the crate root to run, and nothing reports
one that is not: a test that does not exist cannot fail. Splitting a suite is
when the declaration gets forgotten, and this repository splits constantly.

Textual analysis is not enough: files arrive via `mod`, `#[path]` and
`include!`, and the probe example generates its `mod` lines from a macro. This
asks the compiler what it built, via `cargo test -- --list`.

    python3 scripts/check-tests-are-reachable.py
"""

from __future__ import annotations

import os
import pathlib
import re
import subprocess
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from venus_feature import venus_gated_paths, venus_links  # noqa: E402

ROOT = pathlib.Path(__file__).resolve().parent.parent
TOOLCHAIN = "+1.97.0"
TEST_FN = re.compile(r"#\[test\]\s*\n\s*(?:async\s+)?fn\s+(\w+)", re.M)

# Each entry is the cargo invocation that should contain the tests in a subtree.
# (cargo arguments, required). The venus suites link against a virglrenderer
# built outside this repository, so they are optional.
SUITES = [
    (["test", "--workspace", "--locked"], True),
    *[
        (["test", "-p", "bridgevm-hvf", "--example", example, "--locked"], True)
        for example in ("hvf_gic_boot_probe", "hvf_vtimer_cancel_probe", "bridgevm_pc_irq_live", "bridgevm_pc_boot_info_live", "bridgevm_pc_reset_vector_live", "bridgevm_pc_dxe_entry_live")
    ],
    (["test", "-p", "bridgevm-hvf", "--features", "venus", "--lib", "--locked"], False),
    (
        ["test", "-p", "bridgevm-hvf", "--features", "venus", "--example",
         "hvf_gic_boot_probe", "--locked"],
        False,
    ),
]

# The loom models need RUSTFLAGS="--cfg loom" and a release build, which would
# make this gate minutes slower. check-loom.sh already fails when they compile
# to nothing, which is exactly the condition this gate looks for.
EXEMPT = {"crates/bridgevm-hvf/tests/loom_psci.rs", "crates/bridgevm-hvf/tests/loom_reset_generation.rs"}


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
    with_venus = venus_links()
    known: set[str] = set()
    for args, required in SUITES:
        if required or with_venus:
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

    if not with_venus:
        # Only venus-gated files may go unverified; an absent virglrenderer must
        # not excuse an orphan elsewhere, which a first draft of this allowed.
        # A path substring is not enough: env_flag_default_on.rs is declared
        # under #[cfg(feature = "venus")] and its name says nothing about that.
        gated = venus_gated_paths()
        for relative, count in [x for x in unreachable if x[0] in gated]:
            print(f"note: {relative}: {count} unverified (needs venus)")
        unreachable = [x for x in unreachable if x[0] not in gated]

    if unreachable:
        for relative, count in unreachable:
            print(f"{relative}: {count} test(s) never compiled into any binary", file=sys.stderr)
        print(f"tests are reachable: FAIL ({len(unreachable)} file(s))", file=sys.stderr)
        return 1

    print(f"tests are reachable: PASS ({checked} files, {len(known)} tests)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
