"""Owned synthetic archives using public historical T21/T22 receipt data only."""
from __future__ import annotations

import json
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/live-gates"))
import a19_quota_refusal_receipt as quota
import a19_interrupted_restore_receipt as interrupt


def fixture(root: Path, number: int, value: dict | None = None) -> tuple[Path, dict]:
    if value is None:
        name = f"a19-t{number}-r2-20260930-receipt.json"
        value = json.loads((ROOT / "docs/windows-arm/evidence" / name).read_text())
    validator = quota if number == 21 else interrupt
    validator.validate(value, value["commit"])
    job = root / "queue/done" / value["job_id"]
    job.mkdir(parents=True)
    for filename in ("receipt.json", "receipt.public.json"):
        (job / filename).write_text(json.dumps(value) + "\n")
    identity = {key: value[key] for key in ("job_id", "tier", "commit", "input_manifest_sha256")}
    identity["sealed_binary_sha256"] = value["binary_hash"]
    content = "".join(f"{key}={item}\n" for key, item in identity.items())
    (job / "job.env").write_text(content)
    ledger = root / "queue/job-ledger" / job.name / "entry.env"
    ledger.parent.mkdir(parents=True)
    ledger.write_text(content)
    ledger.chmod(0o400)
    validator.validate_seal(value, job)
    return job, value


def read(root: Path, job: Path) -> subprocess.CompletedProcess[bytes]:
    return subprocess.run(
        ["bash", str(ROOT / "scripts/live-gates/bridgevm-live"), "receipt", job.name],
        capture_output=True, timeout=10,
        env=dict(os.environ, BRIDGEVM_LIVE_ROOT=str(root / "queue")),
    )


def replace(path: Path, old: str, new: str) -> None:
    mode = path.stat().st_mode & 0o777
    path.chmod(0o600)
    path.write_text(path.read_text().replace(old, new))
    path.chmod(mode)
