"""Copy the public FD while held open, then validate only immutable queue copies."""
import hashlib
import json
import os
from pathlib import Path
import re

from d11_fixture_files import FileSeal, canonical, read, record, unique
from t12_input_manifest import Inputs, TIER, SHA, parse, binding

IDENTITY = {"job_id", "tier", "commit", "input_manifest_sha256", "sealed_binary_sha256"}


def seal(path, commit, directory, repo):
    with Inputs(path, commit, repo, directory / "hvf_gic_boot_probe") as inputs:
        target = directory / "t12-firmware.fd"
        fd = os.open(target, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o400)
        with os.fdopen(fd, "wb") as output:
            os.lseek(inputs.firmware.fd, 0, os.SEEK_SET)
            while block := os.read(inputs.firmware.fd, 1024 * 1024): output.write(block)
            output.flush(); os.fsync(output.fileno())
        inputs.firmware.check(); inputs.binary.check()
        with FileSeal(target, 64 << 20, inputs.rows["firmware"][1]): pass
        record(directory / "t12-input-seal.json", binding(inputs.data, inputs.rows))


def environment(path):
    result = {}
    for line in read(path, 16384).decode("utf-8").splitlines():
        key, separator, value = line.partition("=")
        if not separator or key in result: raise ValueError("T12 queue identity malformed")
        result[key] = value
    return result


def submitted(directory, commit, name, manifest, binary, repo):
    canonical(directory)
    info = directory.stat()
    if (not directory.is_dir() or info.st_uid != os.geteuid() or info.st_mode & 0o077
            or directory.parent.name != "running" or directory.name != name
            or not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]{0,127}", name)
            or manifest != directory / "input-manifest.tsv" or binary != directory / "hvf_gic_boot_probe"):
        raise ValueError("T12 requires its owned running queue directory")
    ledger = directory.parent.parent / "job-ledger" / name / "entry.env"
    canonical(ledger)
    info = ledger.lstat()
    if info.st_uid != os.geteuid() or info.st_nlink != 1 or info.st_mode & 0o222:
        raise ValueError("T12 ledger is not immutable and owned")
    saved, current = environment(ledger), environment(directory / "job.env")
    if (set(saved) != IDENTITY or any(saved[k] != current.get(k) for k in IDENTITY)
            or saved["commit"] != commit or saved["job_id"] != name or saved["tier"] != TIER
            or any(not SHA.fullmatch(saved[k]) for k in IDENTITY if k.endswith("sha256"))):
        raise ValueError("T12 queue seal differs")
    data = read(manifest, 16384)
    rows = parse(data, commit, repo)
    if (hashlib.sha256(data).hexdigest() != saved["input_manifest_sha256"]
            or rows["binary"][1] != saved["sealed_binary_sha256"]):
        raise ValueError("T12 submitted manifest changed")
    value = json.loads(read(directory / "t12-input-seal.json"), object_pairs_hook=unique)
    if value != binding(data, rows): raise ValueError("T12 copied-input seal changed")
    with FileSeal(binary, 128 << 20, saved["sealed_binary_sha256"]) as runner:
        if not runner.identity[2] & 0o111: raise ValueError("T12 runner not executable")
    with FileSeal(directory / "t12-firmware.fd", 64 << 20, rows["firmware"][1]): pass
    return value
