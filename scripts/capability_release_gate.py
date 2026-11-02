"""Fast development mode (operator, 2026-11-02): a stale tested_commit fails only the release gate."""

from __future__ import annotations

import os
import sys

from capability_freshness import code_changed_since, measured_head_mismatch

__all__ = ["measured_head_mismatch", "stale_tested_commit"]


def stale_tested_commit(tested_commit: str, root) -> str | None:
    """Return the changed path under BRIDGEVM_RELEASE_GATE=1; otherwise only note it."""
    stale = code_changed_since(tested_commit, str(root))
    if stale and os.environ.get("BRIDGEVM_RELEASE_GATE") != "1":
        print(f"capability registry: note: code changed since tested_commit ({stale})", file=sys.stderr)
        return None
    return stale
