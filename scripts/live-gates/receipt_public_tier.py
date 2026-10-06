"""Historical tier hint parsing; extracted without policy changes."""
from pathlib import Path
from native_snapshot_restore_public import load_receipt

def public_tier(path: Path) -> str | None:
    try:
        value = load_receipt(path)
        return value.get("tier") if isinstance(value, dict) else None
    except (OSError, UnicodeError, ValueError, RecursionError):
        return None


