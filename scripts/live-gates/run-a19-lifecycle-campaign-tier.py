#!/usr/bin/env python3
"""Fixed N=10 A19 product lifecycle campaign; sequential lanes, no replacement.

Every lane runs at the job's sealed commit against its sealed input manifest
and probe, with its own disk and vars clones, and must prove its cleanup before
the next lane starts. The first failed lane ends the campaign, as B7 does.
"""
from __future__ import annotations

from datetime import datetime, timezone
import os
from pathlib import Path
import platform
import subprocess
import sys

from a19_lifecycle_campaign_lane import ERRORS, preflight, run_lane
from a19_lifecycle_campaign_read import sealed_hashes
from a19_lifecycle_campaign_receipt import SEALED, aggregate, computed_pass, initial, write_new
from a19_lifecycle_campaign_record import LANES, RECORD, lane_name, lanes_clean, read_records, write_record


def source_commit(repo: Path) -> str:
    return subprocess.check_output(["git", "-C", str(repo), "rev-parse", "HEAD"], text=True).strip()


def host_identity() -> dict[str, str]:
    try:
        model = subprocess.check_output(["sysctl", "-n", "hw.model"], text=True,
                                        stderr=subprocess.DEVNULL).strip()
    except (OSError, subprocess.SubprocessError):
        model = ""
    return {"host_model": model or "absent", "macos_version": platform.mac_ver()[0] or "absent"}


def run_lanes(output: Path, identity: dict, manifest: Path, sealed_binary: Path, repo: Path,
              receipt: dict) -> int:
    """Run lanes 1..10 in order and return how many retained a record."""
    lanes, inputs, written = output / "lanes", None, 0
    receipt.update(outcome="failed", failure_code="internal-error")
    for ordinal in range(1, LANES + 1):
        if os.path.lexists(output / "cancel.requested"):
            receipt.update(outcome="canceled", failure_code="canceled")
            return written
        record, public = run_lane(lanes, ordinal, identity, manifest, sealed_binary, repo)
        write_record(lanes / lane_name(ordinal) / RECORD, record, identity, ordinal)
        written = ordinal
        if public:
            inputs = inputs or public
            if public != inputs:
                raise ValueError("campaign lanes authenticated different sealed inputs")
            receipt.update({field: public[field] for field in SEALED})
        if not record["pass"]:
            # No retry and no replacement lane: the campaign ends here.
            receipt["failure_code"] = "lane-failed" if record["cleanup_verified"] else "cleanup-failed"
            return written
    receipt.update(outcome="completed", failure_code="none")
    return written


def main() -> int:
    if len(sys.argv) != 5:
        print("usage: run-a19-lifecycle-campaign-tier.py OUT JOB_ID MANIFEST SEALED_BINARY", file=sys.stderr)
        return 2
    output, manifest, sealed_binary = map(Path, (sys.argv[1], sys.argv[3], sys.argv[4]))
    job_id, repo = sys.argv[2], Path(__file__).resolve().parents[2]
    commit = source_commit(repo)
    receipt = initial(job_id, commit)
    receipt.update(host_identity())
    identity, written = None, 0
    try:
        try:
            identity = {"job_id": job_id, "commit": commit, **sealed_hashes(output, job_id, commit)}
            receipt.update({field: identity[field] for field in ("input_manifest_sha256", "binary_hash")})
            preflight(manifest, commit, output)
            (output / "lanes").mkdir(mode=0o700)
        except ERRORS:
            receipt.update(outcome="preflight-blocked", failure_code="invalid-input")
            raise
        written = run_lanes(output, identity, manifest, sealed_binary, repo, receipt)
    except ERRORS as error:
        print(f"FAIL: A19 lifecycle campaign: {error}", file=sys.stderr)
    finally:
        collected: list = []
        try:
            collected = read_records(output / "lanes", identity, written) if identity else []
        except ERRORS as error:
            print(f"FAIL: A19 lifecycle campaign records: {error}", file=sys.stderr)
            receipt.update(outcome="failed", failure_code="internal-error")
        aggregate(receipt, collected)
        receipt["worker_cleanup_verified"] = lanes_clean(output)
        if receipt["outcome"] == "completed" and not receipt["worker_cleanup_verified"]:
            receipt.update(outcome="failed", failure_code="cleanup-failed")
        receipt["pass"] = computed_pass(receipt)
        receipt["finished_at"] = datetime.now(timezone.utc).isoformat()
        write_new(output / "receipt.json", receipt)
    return 0 if receipt["pass"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
