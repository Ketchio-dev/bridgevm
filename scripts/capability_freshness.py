"""Fail closed when capability claims outlive checked build and release inputs."""

from __future__ import annotations

import re
import subprocess

# Reproof inputs exclude the registry and its generated documentation.
CODE_PATHS = [
    "crates/", "runners/", "apps/", "scripts/", "tests/", "Cargo.toml", "Cargo.lock",
    ".github/workflows/", ".github/actions/", "install.sh", "deny.toml", "packaging/",
    "tools/", "schemas/", "fuzz/", ".gitattributes", "LICENSE", "THIRD-PARTY-NOTICES.md",
    "THIRD-PARTY-PATCHES.tsv", "docs/licenses/", "docs/machine-contract/qemu-virt-deviations.json",
]


def _git(root: str, *args: str) -> subprocess.CompletedProcess:
    return subprocess.run(["git", *args], cwd=root, capture_output=True, text=True)


def code_changed_since(tested_commit: str, root: str) -> str | None:
    """Return the first changed code path since ``tested_commit``, if any."""
    kind = _git(root, "cat-file", "-t", tested_commit)
    if kind.returncode != 0 or kind.stdout.strip() != "commit":
        # Missing history is missing evidence, even in a shallow checkout.
        return f"tested_commit {tested_commit[:12]} is not an available commit; fetch its history"
    if _git(root, "merge-base", "--is-ancestor", tested_commit, "HEAD").returncode != 0:
        return f"tested_commit {tested_commit[:12]} is not a verified ancestor of HEAD"
    changed = _git(root, "diff", "--name-only", f"{tested_commit}..HEAD", "--", *CODE_PATHS)
    if changed.returncode != 0:
        return f"could not compare tested_commit {tested_commit[:12]} with HEAD"
    return changed.stdout.splitlines()[0] if changed.stdout.strip() else None


def measured_head_mismatch(registry: dict) -> tuple[str, str] | None:
    """Reject a final-head measurement cited against another tested commit."""
    for criterion in registry["criteria"]:
        cited = re.search(r"final head ([0-9a-f]{40})", criterion.get("measured", ""))
        if cited and cited.group(1) != registry["tested_commit"]:
            return criterion["id"], cited.group(1)
    return None
