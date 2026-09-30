"""Summarize the A19 marker lifecycles' retained public evidence."""
from __future__ import annotations

import hashlib
from pathlib import Path
from hvf_run_log import read_run_log
from hvf_terminal_report import LOG_LIMIT, system_off_offset

T20_PHASES = ("phase1-original", "phase3-clobber", "phase5-restored")


def file_digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def line_hash(path: Path) -> str:
    value = path.read_text(encoding="utf-8").strip()
    if len(value) != 64 or any(character not in "0123456789abcdef" for character in value):
        raise ValueError("invalid retained hash")
    return value


def shutdown_count(output: Path, phases: tuple[str, ...]) -> int:
    """Phases whose whole run.log binds the final host report's SYSTEM_OFF stop."""
    return sum(system_off_offset(read_run_log(output / phase / "run.log", LOG_LIMIT - 1)) is not None for phase in phases)
