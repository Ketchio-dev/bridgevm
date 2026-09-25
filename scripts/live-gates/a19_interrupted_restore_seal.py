"""Read the bounded T22 queue seal before touching private media."""
from __future__ import annotations

import os
from pathlib import Path
import re
import stat

TIER = "t22-a19-interrupted-restore"
SHA256 = re.compile(r"[0-9a-f]{64}\Z")
COMMIT = re.compile(r"[0-9a-f]{40}\Z")
JOB_ID = re.compile(r"[A-Za-z0-9][A-Za-z0-9._-]{0,127}\Z")
IDENTITY = ("job_id", "tier", "commit", "input_manifest_sha256", "sealed_binary_sha256")


def present(path: Path) -> bool:
    return os.path.lexists(path)


def read_bounded_regular(path: Path, limit: int, readonly: bool = False) -> bytes:
    descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_CLOEXEC | os.O_NONBLOCK)
    try:
        before = os.fstat(descriptor)
        if (not stat.S_ISREG(before.st_mode) or not 0 < before.st_size <= limit
                or (readonly and before.st_mode & 0o222)):
            raise ValueError("sealed file is not a bounded regular file")
        chunks: list[bytes] = []
        length = 0
        while length <= limit:
            chunk = os.read(descriptor, min(8192, limit + 1 - length))
            if not chunk:
                break
            chunks.append(chunk)
            length += len(chunk)
        after = os.fstat(descriptor)
    finally:
        os.close(descriptor)
    current = os.stat(path, follow_symlinks=False)
    def identity(value: os.stat_result) -> tuple[int, ...]:
        return (value.st_dev, value.st_ino, value.st_mode, value.st_nlink,
                value.st_size, value.st_mtime_ns, value.st_ctime_ns)
    if length != before.st_size or length > limit or identity(before) != identity(after) or identity(after) != identity(current):
        raise ValueError("sealed file changed while it was read")
    return b"".join(chunks)


def _env(path: Path, readonly: bool = False) -> dict[str, str]:
    rows: dict[str, str] = {}
    for line in read_bounded_regular(path, 4096, readonly).decode("utf-8").splitlines():
        key, separator, item = line.partition("=")
        if not separator or key in rows:
            raise ValueError("queue seal has a malformed or repeated field")
        rows[key] = item
    return rows


def sealed_hashes(job_dir: Path, job_id: str, commit: str) -> dict[str, str]:
    if not JOB_ID.fullmatch(job_id) or not COMMIT.fullmatch(commit):
        raise ValueError("T22 queue identity is invalid")
    if job_dir.name != job_id or job_dir.is_symlink() or not job_dir.is_dir():
        raise ValueError("T22 job directory differs from its sealed identity")
    ledger_root = job_dir.parent.parent / "job-ledger"
    ledger_dir = ledger_root / job_id
    if ledger_root.is_symlink() or ledger_dir.is_symlink() or not ledger_dir.is_dir():
        raise ValueError("T22 ledger directory is unsafe")
    job = _env(job_dir / "job.env")
    ledger = _env(ledger_dir / "entry.env", readonly=True)
    if not set(IDENTITY).issubset(job) or set(ledger) != set(IDENTITY):
        raise ValueError("T22 queue seal has missing or unexpected fields")
    for rows in (job, ledger):
        for field, expected in (("job_id", job_id), ("tier", TIER), ("commit", commit)):
            if rows[field] != expected:
                raise ValueError(f"T22 queue seal differs from {field}")
    for field in ("input_manifest_sha256", "sealed_binary_sha256"):
        if not SHA256.fullmatch(job[field]) or ledger[field] != job[field]:
            raise ValueError(f"T22 queue seal differs from {field}")
    return {"input_manifest_sha256": job["input_manifest_sha256"],
            "binary_hash": job["sealed_binary_sha256"]}


def merge_prepared(receipt: dict, public: dict) -> None:
    if any(receipt[field] != public[field] for field in ("input_manifest_sha256", "binary_hash")):
        raise ValueError("prepared inputs differ from the sealed queue hashes")
    receipt.update(public)
