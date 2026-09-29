"""Bind a T20 receipt to its sealed job and original private result."""
from __future__ import annotations

from pathlib import Path
import re

from native_snapshot_restore_public import load_receipt
from native_snapshot_restore_receipt import validate

STAGED_PUBLIC = re.compile(r"\.receipt\.public\.[0-9]+\.json\Z")


def verified_receipt(path: Path, job_dir: Path, expected_commit: str | None) -> dict:
    if path.parent.resolve() != job_dir.resolve():
        raise ValueError("T20 receipt is outside its sealed job directory")
    name = path.name
    if name != "receipt.json" and name != "receipt.public.json" and not STAGED_PUBLIC.fullmatch(name):
        raise ValueError("T20 receipt has an unexpected filename")
    value = validate(load_receipt(path), expected_commit)
    if name != "receipt.json":
        private = validate(load_receipt(job_dir / "receipt.json"), expected_commit)
        if value != private:
            raise ValueError("T20 public receipt differs from its private original")
    return value
