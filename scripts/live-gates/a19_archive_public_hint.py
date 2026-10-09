"""Keep malformed public-only T21/T22 hints on a strict, refusing route."""
import json
from pathlib import Path

from native_snapshot_restore_seal import read_bounded_regular
from receipt_public_tier import public_tier as legacy_public_tier

STRICT_TIERS = ("t21-a19-quota-refusal", "t22-a19-interrupted-restore")


def public_tier(path: Path) -> str | None:
    hints = []

    def collect(pairs):
        hints.extend(value for key, value in pairs if key == "tier")
        return dict(pairs)

    try:
        # Permissive parsing is routing only: duplicate keys and nonfinite
        # values still fail the strict receipt loader before any output.
        json.loads(read_bounded_regular(path, 65_536), object_pairs_hook=collect)
    except (OSError, UnicodeError, ValueError, RecursionError):
        pass
    for tier in STRICT_TIERS:
        if tier in hints:
            return tier
    return legacy_public_tier(path)
