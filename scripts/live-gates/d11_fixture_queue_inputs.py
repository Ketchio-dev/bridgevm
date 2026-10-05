"""Bind D11 to the immutable queue ledger and an exclusive stable output."""
import hashlib
import os
from pathlib import Path

from d11_fixture_files import canonical, read, digest, record
from d11_fixture_inputs import TIER, COMMIT, JOB, SHA, Inputs, parse
from native_snapshot_restore_seal import _env

IDENTITY = {"job_id", "tier", "commit", "input_manifest_sha256", "sealed_binary_sha256"}


def job(directory, commit, expected=None):
    canonical(directory)
    if not directory.is_dir() or directory.stat().st_uid != os.geteuid():
        raise ValueError("fixture queue directory not owned")
    value = _env(directory / "job.env")
    name = value.get("job_id", "")
    if (not JOB.fullmatch(name) or directory.name != name or not COMMIT.fullmatch(commit)
            or value.get("commit") != commit or value.get("tier") != TIER
            or (expected is not None and name != expected)):
        raise ValueError("fixture queue identity differs")
    ledger = directory.parent.parent / "job-ledger" / name / "entry.env"
    canonical(ledger)
    info = ledger.lstat()
    if info.st_uid != os.geteuid() or info.st_nlink != 1 or info.st_mode & 0o222:
        raise ValueError("fixture ledger is not immutable and owned")
    recorded = _env(ledger, readonly=True)
    if set(recorded) != IDENTITY or any(recorded[k] != value.get(k) for k in IDENTITY):
        raise ValueError("fixture ledger seal differs")
    if any(not SHA.fullmatch(recorded[k]) for k in IDENTITY if k.endswith("sha256")):
        raise ValueError("fixture queue hash malformed")
    return recorded


def bound(directory, commit):
    value = job(directory, commit)
    data = read(directory / "input-manifest.tsv")
    rows = parse(data, commit)
    if (hashlib.sha256(data).hexdigest() != value["input_manifest_sha256"]
            or rows["binary"][1] != value["sealed_binary_sha256"]
            or digest(directory / "hvf_gic_boot_probe") != value["sealed_binary_sha256"]):
        raise ValueError("fixture queue content changed")
    return value, rows


def output_path(name):
    if not JOB.fullmatch(name): raise ValueError("invalid fixture output name")
    return Path.home().resolve() / "BridgeVM/d11-fixtures" / name


def create_output(name):
    output = output_path(name)
    parent = output.parent
    parent.mkdir(mode=0o700, parents=False, exist_ok=True)
    canonical(parent)
    info = parent.lstat()
    if (info.st_uid != os.geteuid() or info.st_mode & 0o077
            or info.st_dev != Path.home().stat().st_dev):
        raise ValueError("fixture output parent unsafe")
    output.mkdir(mode=0o700)
    return output


def validate(path, commit):
    with Inputs(path, commit): pass


def seal(path, commit, directory):
    with Inputs(path, commit, directory / "hvf_gic_boot_probe") as inputs:
        # Only hashes and build metadata enter the ledger/public receipt.
        record(directory / "d11-input-seal.private.json", {
            "manifest_sha256": hashlib.sha256(inputs.data).hexdigest(),
            "binary_sha256": inputs.rows["binary"][1]})
        inputs.check()
