#!/usr/bin/env python3
"""Bounded physical-Mac WinPE collection with no GPU acceleration or promotion."""
import argparse
import json
import os
from pathlib import Path
import platform
import subprocess
import sys

from winpe_companion_inputs import COMPANIONS, clone, file_hash, load, verify
from winpe_companion_inspect import compare, inspect
from winpe_companion_mounts import MountSafetyError
from winpe_companion_process import OwnedProcessError
from winpe_companion_receipt import initial, job_fields, validate, write
from winpe_companion_bounds import command, execute, verify as verify_bounds

REPO = Path(__file__).resolve().parents[2]


def run(args):
    out = args.out
    if not out.is_absolute() or not out.is_dir() or out.is_symlink():
        raise ValueError("existing absolute job output required")
    job = job_fields(out); data = initial(job)
    validate(data, job)
    commit = subprocess.check_output(["git", "-C", str(REPO), "rev-parse", "HEAD"], text=True).strip()
    if job["commit"] != commit or args.job_id != job["job_id"]:
        raise ValueError("sealed job mismatch")
    write(out / "receipt.json", data)
    stage = "host"
    try:
        if platform.system() != "Darwin" or platform.machine() != "arm64":
            raise ValueError("physical Apple-silicon Mac required")
        stage = "input"
        if file_hash(args.input_manifest) != job["input_manifest_sha256"]:
            raise ValueError("manifest changed")
        records = load(args.input_manifest, REPO, args.sealed_binary)
        if records["binary"][1] != job.get("sealed_binary_sha256"):
            raise ValueError("binary job seal mismatch")
        private = out / "private"; private.mkdir(mode=0o700, exist_ok=False)
        work = Path.home() / "BridgeVM/work" / ("winpe-companions-" + args.job_id)
        stage = "clone"
        clones = clone(records, work)
        stage = "inspect-before"
        before = inspect(clones["image"], private / "mount-before")
        expected = {name: file_hash(REPO / "scripts/win-assets" / name) for name in COMPANIONS}
        write(private / "before.json", {"observed": before, "expected": expected})
        stage = "execute"
        environment = {key: value for key, value in os.environ.items() if not key.startswith("BRIDGEVM_")}
        environment["BRIDGEVM_PREBUILT_PROBE"] = str(args.sealed_binary)
        data["sample_count"] = 1
        execute(command(REPO, records, clones, private / "boot"), private, environment, data)
        stage = "output-bounds"
        verify_bounds(private)
        data["output_bounds_verified"] = True
        stage = "inspect-after"
        after = inspect(clones["image"], private / "mount-after")
        data["mount_cleanup_complete"] = True
        write(private / "after.json", after)
        data.update(compare(before, after, expected))
        stage = "source-integrity"
        verify(records)
        data["source_integrity_verified"] = True
        write(private / "inputs.json", {key: digest for key, (_, digest) in records.items()})
        write(private / "clones.json", {key: {"path": str(path), "sha256": file_hash(path)}
                                         for key, path in clones.items()})
        for path in clones.values(): path.chmod(0o400)
        data["outcome"] = "diagnostic-complete"
    except (OSError, ValueError, subprocess.SubprocessError, OwnedProcessError, MountSafetyError) as error:
        if isinstance(error, OwnedProcessError): data["cleanup_complete"] = error.cleanup_complete
        if isinstance(error, MountSafetyError): data["mount_cleanup_complete"] = error.cleanup_complete
        if stage == "output-bounds": data["output_bounds_refused"] = True
        data["failure_stage"] = stage
        print("WinPE diagnostic stopped at " + stage + ": " + type(error).__name__, file=sys.stderr)
    finally:
        validate(data, job)
        write(out / "receipt.json", data)
    return 1  # No queue-level acceptance claim, even for a complete collection.


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("out", "job-id", "input-manifest", "sealed-binary"):
        parser.add_argument("--" + name, type=str if name == "job-id" else Path, required=True)
    sys.exit(run(parser.parse_args()))
