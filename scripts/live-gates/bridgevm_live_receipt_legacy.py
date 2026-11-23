"""Serve a published receipt only after checking its queue and ledger identity."""
from __future__ import annotations

import json
import os
from pathlib import Path
import re
import shutil
import sys

from native_snapshot_restore_seal import JOB_ID, _env
from receipt_public_tier import public_tier

LEGACY_COMMIT = re.compile(r"[0-9a-f]{7,40}\Z")


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
    declared = public_tier(public)
    if declared is not None and declared != job["tier"]:
        raise ValueError("receipt tier differs from its job")
    with public.open("rb") as source:
        shutil.copyfileobj(source, sys.stdout.buffer)


if __name__ == "__main__":
    if len(sys.argv) != 3:
        raise SystemExit("usage: bridgevm_live_receipt.py JOB_DIR JOB_ID")
    try:
        serve(Path(sys.argv[1]), sys.argv[2])
    except (OSError, UnicodeError, ValueError, RecursionError) as error:
        raise SystemExit(f"receipt refused: {error}") from error
