"""Summarize the T20 marker lifecycle's retained public evidence."""
from __future__ import annotations

import hashlib
from pathlib import Path


def file_digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def line_hash(path: Path) -> str:
    value = path.read_text(encoding="utf-8").strip()
    if len(value) != 64 or any(character not in "0123456789abcdef" for character in value):
        raise ValueError("invalid retained hash")
    return value


def shutdown_count(output: Path) -> int:
    count = 0
    for phase in ("phase1-original", "phase3-clobber", "phase5-restored"):
        log = output / phase / "run.log"
        if log.is_file() and "stop: PSCI " in log.read_text(encoding="utf-8", errors="replace"):
            count += 1
    return count
