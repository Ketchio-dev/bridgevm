"""Exact development envelope and queue seals; native T22 input schema unchanged."""
import hashlib
import os
from pathlib import Path
import re
import subprocess

from native_snapshot_restore_inputs import FILES, METADATA, parse_manifest, authenticate
from native_snapshot_restore_seal import _env, read_bounded_regular
from t22_pair_provenance import origin, file_hash, SHA, COMMIT
from t22_pair_publication import write_exclusive
from t22_pair_environment import controlled_env

TIER = "d10-t22-owned-pair-preparation"
SCHEMA = "bridgevm.t22-pair-preparation-queue-input.v1"
SCRIPT = "scripts/win-assets/bv-t22-pair-admission.ps1"
EXTRA = {"schema", "origin_manifest", "query_script_sha256"}
IDENTITY = {"job_id", "tier", "commit", "input_manifest_sha256", "sealed_binary_sha256",
            "origin_manifest_sha256", "query_script_sha256"}
JOB = re.compile(r"[A-Za-z0-9][A-Za-z0-9._-]{0,95}\Z")


def parse(data, commit):
    if not 0 < len(data) <= 65_536 or b"\0" in data:
        raise ValueError("invalid development envelope size")
    rows, native = {}, []
    for line in data.decode().splitlines(keepends=True):
        fields = line.rstrip("\r\n").split("\t"); key = fields[0]
        if key in rows or key not in FILES | METADATA | EXTRA:
            raise ValueError("duplicate or unknown development envelope field")
        if len(fields) != (3 if key in FILES | {"origin_manifest"} else 2):
            raise ValueError("invalid development envelope field count")
        rows[key] = fields[1:]
        if key not in EXTRA: native.append(line)
    if set(rows) != FILES | METADATA | EXTRA or rows["schema"] != [SCHEMA]:
        raise ValueError("development envelope schema or coverage mismatch")
    native = "".join(native).encode(); candidate = parse_manifest(native, commit)
    path, digest = rows["origin_manifest"]
    if not Path(path).is_absolute() or str(Path(path).resolve()) != path or not SHA.fullmatch(digest):
        raise ValueError("invalid development origin")
    if not SHA.fullmatch(rows["query_script_sha256"][0]):
        raise ValueError("invalid query script seal")
    return candidate, native, Path(path), digest, rows["query_script_sha256"][0]


def query_hash(root, commit):
    if not COMMIT.fullmatch(commit): raise ValueError("exact development source required")
    data = subprocess.check_output(["/usr/bin/git", "--no-optional-locks", "show",
                                    commit + ":" + SCRIPT], cwd=root, timeout=30, env=controlled_env())
    return hashlib.sha256(data).hexdigest()


def validate(path, commit, root):
    data = read_bounded_regular(path, 65_536)
    rows, native, origin_path, origin_hash, query = parse(data, commit)
    if query != query_hash(root, commit): raise ValueError("query differs from sealed source")
    authenticate(rows, Path(rows["binary"][0]))
    documents = origin(origin_path, origin_hash, rows)
    if read_bounded_regular(path, 65_536) != data: raise ValueError("envelope changed")
    return rows, native, documents, origin_hash, query


def seal(path, commit, directory, root):
    rows, native, documents, origin_hash, query = validate(path, commit, root)
    write_exclusive(directory / "retained-origin.json", documents[0].data)
    (directory / "retained-origin.json").chmod(0o400)
    for document in documents: document.require_unchanged()
    data = f"origin_manifest_sha256={origin_hash}\nquery_script_sha256={query}\n".encode()
    write_exclusive(directory / "input-asset-seal.env", data)
    (directory / "input-asset-seal.env").chmod(0o400)


def job(directory, commit, job_id=None):
    if directory.resolve() != directory or not directory.is_dir() or directory.stat().st_uid != os.geteuid():
        raise ValueError("development job directory is not owned and canonical")
    job = _env(directory / "job.env")
    name = job.get("job_id", "")
    if (not JOB.fullmatch(name) or directory.name != name or directory.is_symlink()
            or job.get("tier") != TIER or job.get("commit") != commit or not COMMIT.fullmatch(commit)
            or (job_id is not None and job_id != name)):
        raise ValueError("development job identity mismatch")
    ledger = directory.parent.parent / "job-ledger" / name
    if ledger.is_symlink() or ledger.parent.is_symlink(): raise ValueError("unsafe development ledger")
    recorded = _env(ledger / "entry.env", readonly=True)
    info = (ledger / "entry.env").lstat()
    if info.st_uid != os.geteuid() or info.st_nlink != 1: raise ValueError("development ledger is aliased or unowned")
    if set(recorded) != IDENTITY or not IDENTITY.issubset(job): raise ValueError("incomplete development seal")
    for key in IDENTITY:
        if recorded[key] != job[key]: raise ValueError("development ledger differs from job")
        if key.endswith("sha256") and not SHA.fullmatch(job[key]): raise ValueError("invalid development hash")
    return {key: job[key] for key in IDENTITY}


def bound(directory, root, commit):
    identity = job(directory, commit)
    source = directory / "input-manifest.tsv"
    data = read_bounded_regular(source, 65_536)
    if hashlib.sha256(data).hexdigest() != identity["input_manifest_sha256"]:
        raise ValueError("development envelope seal mismatch")
    rows, native, _, origin_hash, query = parse(data, commit)
    if (origin_hash != identity["origin_manifest_sha256"] or query != identity["query_script_sha256"]
            or query != query_hash(root, commit) or file_hash(root / SCRIPT) != query
            or rows["binary"][1] != identity["sealed_binary_sha256"]
            or file_hash(directory / "hvf_gic_boot_probe") != identity["sealed_binary_sha256"]):
        raise ValueError("development source or binary seals differ")
    authenticate(rows, directory / "hvf_gic_boot_probe")
    documents = origin(directory / "retained-origin.json", origin_hash, rows)
    for document in documents: document.require_unchanged()
    return identity, rows, native, documents


def stable_output(job_id):
    if not JOB.fullmatch(job_id): raise ValueError("invalid preparation output identity")
    return Path.home().resolve() / "BridgeVM/t22-prepared-pairs" / job_id
