"""Exact-source B9 file reads and mutable guest-share checks."""

from __future__ import annotations

import hashlib
import os
from pathlib import Path
import stat
import subprocess


REPO = Path(__file__).resolve().parents[2]
CHUNK_BYTES = 7_500_000
DIRECT = {"media": "bbb_1080p_10s_5MB_av1.webm",
          "presentmon": "PresentMon-2.5.1-x64.exe",
          "guest_script": "bv-b9-vlc-playback.ps1",
          "guest_helper_script": "bv-b9-private-inputs.ps1",
          "control_script": "bv-b9-control.ps1"}


def parent_chain(path: Path) -> None:
    """Refuse aliases in every existing ancestor, not only the leaf."""
    if not path.is_absolute() or str(path) != str(Path(os.path.normpath(path))):
        raise ValueError("input path must be absolute and normalized")
    for parent in (path, *path.parents):
        if stat.S_ISLNK(os.lstat(parent).st_mode):
            raise ValueError("symlink in input path")


def stable_file(path: Path, *, maximum: int | None = None) -> tuple[int, str]:
    parent_chain(path)
    flags = os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK
    with os.fdopen(os.open(path, flags), "rb") as stream:
        before = os.fstat(stream.fileno())
        if not stat.S_ISREG(before.st_mode) or before.st_size <= 0:
            raise ValueError("input must be a nonempty regular file")
        if maximum is not None and before.st_size > maximum:
            raise ValueError("input exceeds size bound")
        digest = hashlib.sha256()
        count = 0
        for block in iter(lambda: stream.read(8 * 1024 * 1024), b""):
            count += len(block)
            if count > before.st_size or (maximum is not None and count > maximum):
                raise ValueError("input grew while hashing")
            digest.update(block)
        after = os.fstat(stream.fileno())
    identity = lambda x: (x.st_dev, x.st_ino, x.st_size, x.st_mtime_ns, x.st_ctime_ns)
    if count != before.st_size or identity(before) != identity(after) or identity(after) != identity(os.lstat(path)):
        raise ValueError("input changed while hashing")
    return before.st_size, digest.hexdigest()


def bounded_bytes(path: Path, limit: int) -> bytes:
    parent_chain(path)
    flags = os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK
    with os.fdopen(os.open(path, flags), "rb") as stream:
        before = os.fstat(stream.fileno())
        if not stat.S_ISREG(before.st_mode) or not 0 < before.st_size <= limit:
            raise ValueError("metadata must be a bounded regular file")
        raw = stream.read(limit + 1)
        after = os.fstat(stream.fileno())
    identity = lambda x: (x.st_dev, x.st_ino, x.st_size, x.st_mtime_ns, x.st_ctime_ns)
    if (len(raw) != before.st_size or identity(before) != identity(after)
            or identity(after) != identity(os.lstat(path))):
        raise ValueError("metadata changed while reading")
    return raw


def committed_blob_sha256(path: Path, root: Path = REPO) -> str:
    raw = subprocess.check_output(["/usr/bin/git", "-C", str(root), "show",
                                   "HEAD:" + path.relative_to(root).as_posix()])
    return hashlib.sha256(raw).hexdigest()


def check_shared_assets(records: dict, staged: dict, share: Path) -> None:
    """Compare source, initial stage and current share at each host boundary."""
    seen: set[tuple[int, int]] = set()
    for key, name in DIRECT.items():
        source, expected = records[key]
        target = share / name
        if source.name != name or stable_file(source, maximum=CHUNK_BYTES)[1] != expected:
            raise ValueError("B9 sealed source asset differs: " + key)
        observed = stable_file(target, maximum=CHUNK_BYTES)
        metadata = os.lstat(target)
        inode = (metadata.st_dev, metadata.st_ino)
        if metadata.st_nlink != 1 or inode in seen:
            raise ValueError("B9 shared asset aliases another file: " + key)
        seen.add(inode)
        if observed != staged.get(name) or observed[1] != expected:
            raise ValueError("B9 mutable shared asset differs: " + key)
