#!/usr/bin/env python3
"""Fail when a current document points at something that does not exist.

A document that names a flag nobody implements, or a commit nobody can fetch,
sends the reader somewhere real-looking and empty. Three references are checked
because each has been wrong in this repository at least once:

  * command-line flags, which drift when a script is renamed or a mode removed,
  * repository paths, which break silently when a file moves,
  * commit ids, which are unverifiable once a branch is gone.

docs/archive and docs/handoffs are point-in-time records: their references
described the tree on the day they were written and are not expected to hold
now. Commit ids from other projects are accepted when the surrounding lines say
whose they are, because this repository cannot resolve them either way.

Commit checking needs full history and so is skipped on a shallow clone, where
every older id looks missing. Build outputs are skipped too: they exist on a
machine that has built and nowhere else, so requiring them would fail a clean
checkout for being clean.
"""

from __future__ import annotations

import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
HISTORICAL = ("docs/archive/", "docs/handoffs/")
# "Ketchio-dev" alone is not evidence of an external commit: it owns this
# repository too, and as a bare term it matched the copyright line and excused
# any invented id placed near it. The fork forms stay because they name another
# repository explicitly.
EXTERNAL_SOURCE = re.compile(
    r"fork|ppsspp|mesa|edk2|akre@|upstream|Ketchio-dev/[a-z]|virglrenderer|qemu"
    r"|tianocore|arehnman|secureboot_objects|kvm-guest-drivers|Microsoft"
    r"|Builder commit|source SHA|source_ref|driver ",
    re.I,
)
# Flags of programs this repository runs but does not implement: cargo and
# SwiftPM, and the PPSSPP title used as the A2/A3 workload.
EXTERNAL_FLAGS = {
    "all-targets", "all-features", "no-default-features", "build-tests",
    "package-path", "allow-dirty", "allow-staged", "locked", "release",
    "dry-run", "help", "version",
    "touchscreentest", "vulkan-available-check",
}
FLAG = re.compile(r"--([a-z][a-z0-9-]{3,})")
# Produced by a build, never committed.
GENERATED = ("apps/macos/.build", "target/", ".build/")
REPO_PATH = re.compile(
    r"`((?:scripts|crates|apps|docs|capabilities|schemas|tests|runners|\.github)"
    r"/[A-Za-z0-9_./-]+)`"
)
SHA = re.compile(r"\b([0-9a-f]{40})\b")


def tracked(pattern: str) -> list[str]:
    out = subprocess.run(
        ["git", "ls-files", pattern], cwd=ROOT, capture_output=True, text=True
    )
    return out.stdout.split()


def shallow() -> bool:
    out = subprocess.run(
        ["git", "rev-parse", "--is-shallow-repository"],
        cwd=ROOT, capture_output=True, text=True,
    )
    return out.stdout.strip() == "true"


def main() -> int:
    docs = [d for d in tracked("*.md") if not d.startswith(HISTORICAL)]
    check_commits = not shallow()
    sources = subprocess.run(
        ["bash", "-c", "git ls-files '*.rs' '*.sh' '*.py' '*.swift' '*.ps1' | xargs cat"],
        cwd=ROOT, capture_output=True, text=True,
    ).stdout

    failures: list[str] = []
    for doc in docs:
        path = os.path.join(ROOT, doc)
        lines = open(path, errors="ignore").read().splitlines()
        for number, line in enumerate(lines, 1):
            if re.search(r"`[^`]*--[a-z]", line):
                for match in FLAG.finditer(line):
                    flag = match.group(1)
                    if flag in EXTERNAL_FLAGS:
                        continue
                    if flag in sources or flag.replace("-", "_") in sources:
                        continue
                    failures.append(f"{doc}:{number}: no code defines --{flag}")

            for match in REPO_PATH.finditer(line):
                target = match.group(1).rstrip(".")
                if target.endswith("/"):
                    continue
                if target.startswith(GENERATED):
                    continue
                if not os.path.exists(os.path.join(ROOT, target)):
                    failures.append(f"{doc}:{number}: path does not exist: {target}")

            if not check_commits:
                continue
            for match in SHA.finditer(line):
                commit = match.group(1)
                exists = subprocess.run(
                    ["git", "cat-file", "-e", commit + "^{commit}"],
                    cwd=ROOT, capture_output=True,
                )
                if exists.returncode == 0:
                    continue
                context = " ".join(lines[max(0, number - 4):number])
                if EXTERNAL_SOURCE.search(context):
                    continue
                failures.append(
                    f"{doc}:{number}: commit {commit[:12]} is not in this "
                    "repository and no nearby line says whose it is"
                )

    if failures:
        for failure in failures:
            print(f"documentation references: {failure}", file=sys.stderr)
        print(
            f"documentation references: FAIL ({len(failures)} broken)",
            file=sys.stderr,
        )
        return 1

    scope = "flags, paths and commits" if check_commits else "flags and paths"
    print(f"documentation references: PASS ({len(docs)} current documents, {scope})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
