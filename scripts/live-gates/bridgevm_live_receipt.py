"""Serve T20 and T23 receipts only after checking their current queue and content seals."""
from __future__ import annotations

import json
import os
from pathlib import Path
import re
import shutil
import sys

from a19_lifecycle_campaign_read import strict_reader
from native_snapshot_restore_public import load_receipt
from native_snapshot_restore_seal import JOB_ID, _env

LEGACY_COMMIT = re.compile(r"[0-9a-f]{7,40}\Z")


def public_tier(path: Path) -> str | None:
    try:
        value = load_receipt(path)
        return value.get("tier") if isinstance(value, dict) else None
    except (OSError, UnicodeError, ValueError, RecursionError):
        return None


def serve(directory: Path, job_id: str) -> None:
    if not JOB_ID.fullmatch(job_id) or directory.name != job_id:
        raise ValueError("receipt job id is not canonical")
    public = directory / "receipt.public.json"
    job = _env(directory / "job.env")
    if job.get("job_id") != job_id or not LEGACY_COMMIT.fullmatch(job.get("commit", "")) or not job.get("tier"):
        raise ValueError("receipt job identity is incomplete")
    ledger_dir = directory.parent.parent / "job-ledger" / job_id
    ledger_path = ledger_dir / "entry.env"
    ledger = None
    if ledger_dir.is_symlink() or ledger_dir.parent.is_symlink():
        raise ValueError("receipt ledger path is unsafe")
    if os.path.lexists(ledger_path):
        ledger = _env(ledger_path, readonly=True)
        if any(ledger.get(field) != job[field] for field in ("job_id", "tier", "commit")):
            raise ValueError("receipt job identity differs from its ledger")
    tiers = (job["tier"], ledger["tier"] if ledger else None, public_tier(public))
    reader = strict_reader(tiers)
    if reader is not None:
        if directory.parent.name != "done" or directory.parent.is_symlink():
            raise ValueError("strict T20/T23 receipt is not in the done queue")
        value = reader(public, directory)
        json.dump(value, sys.stdout, indent=2, sort_keys=True)
        sys.stdout.write("\n")
        return
    # Preserve the existing byte-for-byte CLI behavior for other live tiers.
    with public.open("rb") as source:
        shutil.copyfileobj(source, sys.stdout.buffer)


if __name__ == "__main__":
    if len(sys.argv) != 3:
        raise SystemExit("usage: bridgevm_live_receipt.py JOB_DIR JOB_ID")
    try:
        serve(Path(sys.argv[1]), sys.argv[2])
    except (OSError, UnicodeError, ValueError) as error:
        raise SystemExit(f"receipt refused: {error}") from error
