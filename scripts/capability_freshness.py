"""Checks that a capability claim still describes the tree it is read against.

Both failures here are silent by nature: the registry stays valid JSON, every
criterion still says PROVEN, and only the commit it was proven at has moved on.
"""

from __future__ import annotations

import re
import subprocess

# Directories whose contents a capability claim is about. Changes to the
# registry and to the blocks generated from it are deliberately excluded:
# requiring the file to name the commit that changes the file cannot converge.
CODE_PATHS = ["crates/", "runners/", "apps/", "scripts/", "tests/"]


def code_changed_since(tested_commit: str, root: str) -> str | None:
    """Return the first changed code path since ``tested_commit``, if any."""
    changed = subprocess.run(
        ["git", "diff", "--name-only", f"{tested_commit}..HEAD", "--", *CODE_PATHS],
        cwd=root,
        capture_output=True,
        text=True,
    )
    # A non-zero status means a shallow or unrelated checkout, not a violation.
    if changed.returncode != 0 or not changed.stdout.strip():
        return None
    return changed.stdout.split()[0]


def measured_head_mismatch(registry: dict) -> tuple[str, str] | None:
    """Return (criterion id, cited head) when a measured entry names another head.

    A measured entry that cites a different head was copied forward from an
    older run, which is how a stale log hash survives a re-proof.
    """
    for criterion in registry["criteria"]:
        cited = re.search(r"final head ([0-9a-f]{40})", criterion.get("measured", ""))
        if cited and cited.group(1) != registry["tested_commit"]:
            return criterion["id"], cited.group(1)
    return None
