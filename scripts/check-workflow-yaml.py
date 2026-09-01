#!/usr/bin/env python3
"""Fail when a GitHub Actions workflow does not parse, or loses a required job.

A workflow that does not parse does not run, and GitHub reports that on the
push rather than before it. Breaking ci.yml -- replacing a `run:` value with a
YAML flow sequence, keeping the line count identical -- left the whole local
check reporting PASS.

Parsing is the point, so this needs a YAML parser. Where one is absent the check
reports SKIP locally and fails in CI, on the same reasoning as the shellcheck
gate: a check that quietly does nothing is worse than none, because it converts
an unexamined file into a claim.

    python3 scripts/check-workflow-yaml.py
"""

from __future__ import annotations

import os
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
WORKFLOWS = ROOT / ".github/workflows"

# Jobs whose loss would silently remove coverage this project relies on. Each
# was added because something it checks had none.
REQUIRED_JOBS = {
    "ci.yml": {"fmt", "clippy", "test", "budgets", "swift", "truth", "linux-stubs", "windows-nvme-workload"},
    "release.yml": {"artifacts"}, "hvf-performance-binary.yml": {"build-attest"},
    "security-quality.yml": {"supply-chain", "fuzz-smoke", "loom"},
}

def main() -> int:
    try:
        import yaml
    except ModuleNotFoundError:
        if os.environ.get("CI"):
            print("workflow yaml: FAIL (PyYAML is required here)", file=sys.stderr)
            return 1
        print("workflow yaml: SKIP (PyYAML not installed)")
        return 0

    problems: list[str] = []
    checked = 0
    for path in sorted(WORKFLOWS.glob("*.yml")):
        checked += 1
        try:
            document = yaml.safe_load(path.read_text())
        except yaml.YAMLError as error:
            problems.append(f"{path.name}: does not parse: {str(error).splitlines()[0]}")
            continue
        jobs = set((document or {}).get("jobs", {}))
        missing = REQUIRED_JOBS.get(path.name, set()) - jobs
        for job in sorted(missing):
            problems.append(f"{path.name}: required job '{job}' is gone")

    if problems:
        for problem in problems:
            print(problem, file=sys.stderr)
        print(f"workflow yaml: FAIL ({len(problems)})", file=sys.stderr)
        return 1

    if checked == 0:
        print("workflow yaml: FAIL (no workflows found)", file=sys.stderr)
        return 1

    print(f"workflow yaml: PASS ({checked} workflows)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
