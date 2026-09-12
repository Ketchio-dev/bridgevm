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
from winpe_companion_receipt import initial, job_fields, validate, write

REPO = Path(__file__).resolve().parents[2]


def command(repo, records, clones, evidence):
    return ["bash", str(repo / "scripts/run-hvf-windows-installed-boot.sh"),
            "--target", str(clones["image"]), "--vars", str(clones["vars"]),
            "--placeholder-nsid1", str(clones["injector"]),
            "--firmware-code", str(records["firmware"][0]), "--evidence-dir", str(evidence),
            "--release", "--skip-build", "--watchdog-ms", "300000", "--max-reboots", "0",
            "--ram-mib", "4096", "--smp-cpus", "4", "--max-exits", "50000000",
            "--ramfb-samples", "1000,15000,30000,60000,90000,110000,120000",
            "--display-export-ppm", str(evidence / "latest.ppm"), "--no-guest-disk-harvest"]


def run(args):
    out = args.out
    if not out.is_absolute() or not out.is_dir() or out.is_symlink():
        raise ValueError("existing absolute job output required")
    job = job_fields(out)
    data = initial(job)
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
        private = out / "private"
        private.mkdir(mode=0o700, exist_ok=False)
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
        with (private / "wrapper.log").open("x") as log:
            result = subprocess.run(command(REPO, records, clones, private / "boot"),
                                    env=environment, stdout=log, stderr=subprocess.STDOUT, timeout=360)
        data["execution_exit_code"] = result.returncode
        stage = "inspect-after"
        after = inspect(clones["image"], private / "mount-after")
        write(private / "after.json", after)
        data.update(compare(before, after, expected))
        stage = "source-integrity"
        verify(records)
        data["source_integrity_verified"] = True
        write(private / "inputs.json", {key: digest for key, (_, digest) in records.items()})
        write(private / "clones.json", {key: {"path": str(path), "sha256": file_hash(path)}
                                         for key, path in clones.items()})
        for path in clones.values():
            path.chmod(0o400)
        data["outcome"] = "diagnostic-complete"
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        data["failure_stage"] = stage
        print("WinPE diagnostic stopped at " + stage + ": " + type(error).__name__, file=sys.stderr)
    finally:
        validate(data, job)
        write(out / "receipt.json", data)
    return 1  # No queue-level acceptance claim, even for a complete collection.


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--job-id", required=True)
    parser.add_argument("--input-manifest", type=Path, required=True)
    parser.add_argument("--sealed-binary", type=Path, required=True)
    sys.exit(run(parser.parse_args()))
