"""Checks that a capability claim still describes the tree it is read against.
All failures here are silent by nature: the registry stays valid JSON and every
criterion still says PROVEN; only the commit it was proven at has moved on.
"""

from __future__ import annotations

import re
import subprocess

# Directories a capability claim is about; the registry and its generated
# blocks are excluded (naming the commit that changes the file cannot converge).
CODE_PATHS = ["crates/", "runners/", "apps/", "scripts/", "tests/"]


def _git(root: str, *args: str) -> subprocess.CompletedProcess:
    return subprocess.run(["git", *args], cwd=root, capture_output=True, text=True)


def code_changed_since(tested_commit: str, root: str) -> str | None:
    """Return the first changed code path since ``tested_commit``, if any."""
    if _git(root, "cat-file", "-t", tested_commit).returncode != 0:
        # In a full clone an unknown id is a registry defect (a mistyped seal
        # passed silently on 2026-08-20); only a shallow checkout may skip it.
        if _git(root, "rev-parse", "--is-shallow-repository").stdout.strip() != "true":
            return f"tested_commit {tested_commit[:12]} is not a known commit"
        return None
    changed = _git(root, "diff", "--name-only", f"{tested_commit}..HEAD", "--", *CODE_PATHS)
    if changed.returncode != 0 or not changed.stdout.strip():
        return None
    return changed.stdout.split()[0]


def measured_head_mismatch(registry: dict) -> tuple[str, str] | None:
    """Return (criterion id, cited head) when a measured entry names another
    head: it was copied forward, which lets a stale log hash survive a re-proof.
    """
    for criterion in registry["criteria"]:
        cited = re.search(r"final head ([0-9a-f]{40})", criterion.get("measured", ""))
        if cited and cited.group(1) != registry["tested_commit"]:
            return criterion["id"], cited.group(1)
    return None
