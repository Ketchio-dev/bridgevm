"""Bounded private copies of B9 diagnostic files after a guest run."""

from __future__ import annotations

import hashlib
import os
from pathlib import Path
import stat

from b9_share_asset_integrity import parent_chain, stable_file

HOST_LIMIT = 512 * 1024 * 1024
GUEST_JSON_LIMIT = 8192
CSV_LIMIT = 7_500_000


def _identity(item) -> tuple[int, int, int, int, int]:
    return item.st_dev, item.st_ino, item.st_size, item.st_mtime_ns, item.st_ctime_ns


def _copy_one(source: Path, target: Path, limit: int) -> dict | None:
    try:
        before = os.lstat(source)
    except FileNotFoundError:
        return None
    parent_chain(source)
    if not stat.S_ISREG(before.st_mode) or before.st_nlink != 1:
        raise ValueError("B9 raw source must be an unaliased regular file")
    if before.st_size == 0:
        return None
    if before.st_size > limit:
        raise ValueError("B9 raw source exceeds capture bound")
    source_fd = os.open(source, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    created = False
    try:
        with os.fdopen(source_fd, "rb") as incoming:
            opened = os.fstat(incoming.fileno())
            if not stat.S_ISREG(opened.st_mode) or opened.st_nlink != 1 or _identity(opened) != _identity(before):
                raise ValueError("B9 raw source changed before copy")
            target_fd = os.open(target, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
            created = True
            digest = hashlib.sha256()
            count = 0
            with os.fdopen(target_fd, "wb") as outgoing:
                for block in iter(lambda: incoming.read(1024 * 1024), b""):
                    count += len(block)
                    if count > limit or count > opened.st_size:
                        raise ValueError("B9 raw source grew during copy")
                    outgoing.write(block)
                    digest.update(block)
            if (count != opened.st_size or _identity(opened) != _identity(os.fstat(incoming.fileno()))
                    or _identity(opened) != _identity(os.lstat(source))):
                raise ValueError("B9 raw source changed during copy")
        if stable_file(target, maximum=limit) != (count, digest.hexdigest()):
            raise ValueError("B9 raw private copy differs")
        return {"bytes": count, "sha256": digest.hexdigest()}
    except BaseException:
        if created:
            target.unlink(missing_ok=True)
        raise


def copy_raw(work: Path, raw: Path, share: Path, boot: Path, nonce: str) -> dict:
    """Copy existing nonempty inputs; reject unsafe inputs before reporting them."""
    raw.mkdir(mode=0o700, exist_ok=False)
    selected = ((boot / "run.log", "run.log", HOST_LIMIT),
                (work / "launcher.log", "launcher.log", HOST_LIMIT),
                (boot / "virtio-gpu.jsonl", "virtio-gpu.jsonl", HOST_LIMIT),
                (share / ("ready-" + nonce + ".json"), "guest-ready.json", GUEST_JSON_LIMIT),
                (share / ("collector-" + nonce + ".json"), "guest-collector.json", GUEST_JSON_LIMIT),
                (share / ("finished-" + nonce + ".json"), "guest-finished.json", GUEST_JSON_LIMIT),
                (share / ("b9-" + nonce + ".csv"), "presentmon.csv", CSV_LIMIT))
    result = {}
    for source, name, limit in selected:
        copied = _copy_one(source, raw / name, limit)
        if copied is not None:
            result[name] = copied
    return result
