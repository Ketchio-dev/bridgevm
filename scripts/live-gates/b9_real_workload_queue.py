#!/usr/bin/env python3
"""Exact-source queue adapter for one nonpromoting B9 playback diagnostic."""

from __future__ import annotations

import json
import os
from pathlib import Path
import subprocess
import sys

from b9_real_workload_inputs import KEYS
from b9_real_workload_receipt import (FLAGS, TIER, job_fields, public_view,
                                      read_json, validate_private)


def write_exclusive(path: Path, value: dict) -> None:
    data = (json.dumps(value, sort_keys=True, indent=2, allow_nan=False) + "\n").encode()
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    with os.fdopen(fd, "wb") as output:
        output.write(data)


def missing(job: dict) -> dict:
    return {
        "schema": "bridgevm.b9-real-workload-pilot.v1", "tier": TIER,
        "criterion": "B9", "candidate_id": "vlc-3.0.23-arm64-kodi-bbb-1080p-av1-10s-v1",
        "job_id": job["job_id"], "commit": job["commit"],
        "input_manifest_sha256": job["input_manifest_sha256"],
        "sealed_binary_sha256": job["sealed_binary_sha256"],
        "asset_hashes": {key: job["asset_" + key + "_sha256"] for key in KEYS},
        "outcome": "diagnostic-incomplete", "result_class": "INVALID_EVIDENCE",
        "pilot_count": 0, "required_workload_count": 20, "frame_count": 0,
        "scanout_sample_count": 0, "cleanup_complete": False,
        "source_integrity": False, "owned_process_group_stopped": False,
        "owned_vm_pgid": 0, "nonce_sha256": "0" * 64,
        **{flag: False for flag in FLAGS},
    }


def finalize(directory: Path, commit: str) -> None:
    job = job_fields(directory)
    if job["commit"] != commit:
        raise ValueError("B9 finalize source differs")
    target = directory / "receipt.json"
    if target.exists() or target.is_symlink():
        return
    diagnostic = directory / "diagnostic"
    source = diagnostic / "receipt.json"
    value = read_json(source) if source.is_file() else missing(job)
    if (directory / "cancel.requested").is_file():
        value["outcome"] = "canceled"
        value["result_class"] = "PLAYBACK_INCOMPLETE"
    if source.is_file():
        validate_private(value, job, diagnostic)
    write_exclusive(target, value)


def publish(directory: Path, commit: str) -> None:
    job = job_fields(directory)
    if job["commit"] != commit:
        raise ValueError("B9 publication source differs")
    value = read_json(directory / "receipt.json")
    validate_private(value, job, directory / "diagnostic")
    write_exclusive(directory / "receipt.public.json", public_view(value, job))


def run(directory: Path, repo: Path, commit: str, manifest: Path, binary: Path) -> int:
    job = job_fields(directory)
    if job["commit"] != commit or repo.resolve() != Path(__file__).resolve().parents[2]:
        raise ValueError("B9 queue source differs")
    diagnostic = directory / "diagnostic"
    diagnostic.mkdir(mode=0o700, exist_ok=False)
    subprocess.run([sys.executable, str(repo / "scripts/live-gates/run-b9-real-workload-pilot.py"),
                    "--out", str(diagnostic), "--job-id", job["job_id"],
                    "--input-manifest", str(manifest), "--sealed-binary", str(binary)],
                   check=False)
    finalize(directory, commit)
    return 1  # A pilot is never a queue-level criterion pass.


def main() -> int:
    if len(sys.argv) < 5:
        raise ValueError("B9 queue mode and exact job source required")
    mode, raw_directory, raw_repo, commit = sys.argv[1:5]
    directory = Path(raw_directory).resolve()
    try:
        if mode == "run":
            if len(sys.argv) != 7:
                raise ValueError("B9 run needs repository, manifest and binary")
            return run(directory, Path(raw_repo), commit, Path(sys.argv[5]), Path(sys.argv[6]))
        if mode == "finalize":
            if len(sys.argv) != 5:
                raise ValueError("B9 finalize has unexpected arguments")
            finalize(directory, commit)
        elif mode == "publish":
            if len(sys.argv) != 5:
                raise ValueError("B9 publish has unexpected arguments")
            publish(directory, commit)
        else:
            raise ValueError("unknown B9 queue adapter mode")
    except (OSError, ValueError) as error:
        print("B9 queue adapter refused: " + str(error), file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
