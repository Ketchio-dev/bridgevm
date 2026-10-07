"""Common file digest semantics for sealed product receipts."""
import hashlib
from pathlib import Path

def digest(path: Path) -> str:
    if not path.is_file() or path.is_symlink():
        return "absent"
    value = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            value.update(chunk)
    return value.hexdigest()
