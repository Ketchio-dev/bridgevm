"""Bounded, no-symlink file reads and copies for offline B8 contracts."""
from __future__ import annotations

import hashlib
import os
from pathlib import Path
import re
import stat
import struct
import subprocess


def _canonical(path: Path) -> None:
    if not path.is_absolute() or str(path) != os.path.normpath(str(path)):
        raise ValueError("B8 path must be absolute and normalized")
    for parent in (path, *path.parents):
        if stat.S_ISLNK(os.lstat(parent).st_mode):
            raise ValueError("B8 path has a symlink ancestor")


def _identity(item) -> tuple[int, ...]:
    return (item.st_dev, item.st_ino, item.st_mode, item.st_nlink,
            item.st_size, item.st_mtime_ns, item.st_ctime_ns)


def _source(path: Path, limit: int):
    _canonical(path)
    descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | getattr(os, "O_CLOEXEC", 0) | getattr(os, "O_NONBLOCK", 0))
    stream = os.fdopen(descriptor, "rb")
    before = os.fstat(descriptor)
    if not stat.S_ISREG(before.st_mode) or not 0 < before.st_size <= limit or before.st_nlink != 1:
        stream.close()
        raise ValueError("B8 input is not a bounded regular file")
    return stream, before


def _unchanged(path: Path, stream, before) -> None:
    if _identity(before) != _identity(os.fstat(stream.fileno())) or _identity(before) != _identity(os.lstat(path)):
        raise ValueError("B8 source changed during read")


def read_regular(path: Path, limit: int) -> bytes:
    stream, before = _source(path, limit)
    with stream:
        raw = stream.read(limit + 1)
        if len(raw) != before.st_size:
            raise ValueError("B8 input changed during read")
        _unchanged(path, stream, before)
    return raw


def _env(path: Path, readonly: bool = False) -> dict[str, str]:
    raw = read_regular(path, 8192)
    if readonly and os.lstat(path).st_mode & 0o222:
        raise ValueError("B8 ledger is writable")
    fields: dict[str, str] = {}
    for line in raw.decode("ascii").splitlines():
        key, separator, item = line.partition("=")
        if not separator or key in fields or not re.fullmatch(r"[a-z0-9_]+", key):
            raise ValueError("B8 job seal has a malformed field")
        fields[key] = item
    return fields


def _hash(path: Path, limit: int) -> str:
    stream, before = _source(path, limit)
    digest = hashlib.sha256()
    with stream:
        count = 0
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            count += len(block)
            if count > before.st_size or count > limit:
                raise ValueError("B8 asset grew during hashing")
            digest.update(block)
        if count != before.st_size:
            raise ValueError("B8 asset length changed during hashing")
        _unchanged(path, stream, before)
    return digest.hexdigest()


def _copy_tar(path: Path, destination: Path, expected: str, limit: int) -> None:
    stream, before = _source(path, limit)
    created = False
    try:
        with stream, destination.open("xb") as output:
            created = True
            digest = hashlib.sha256()
            count = 0
            for block in iter(lambda: stream.read(1024 * 1024), b""):
                count += len(block)
                if count > before.st_size or count > limit:
                    raise ValueError("B8 release tarball grew during copy")
                output.write(block)
                digest.update(block)
            if count != before.st_size:
                raise ValueError("B8 release tarball length changed")
            _unchanged(path, stream, before)
        if (destination.stat().st_size != count or digest.hexdigest() != expected
                or _hash(destination, limit) != expected):
            raise ValueError("B8 release tarball differs from sealed bytes")
    except BaseException:
        if created:
            destination.unlink(missing_ok=True)
        raise


def read_committed_blob(root: Path, commit: str, path: str, limit: int) -> bytes:
    selector = f"{commit}:{path}"
    env = {"PATH": "/usr/bin:/bin:/usr/sbin:/sbin", "HOME": "/dev/null",
           "GIT_CONFIG_NOSYSTEM": "1", "GIT_CONFIG_GLOBAL": "/dev/null",
           "GIT_NO_REPLACE_OBJECTS": "1", "GIT_TERMINAL_PROMPT": "0"}
    def fixed_git(*args):
        try:
            return subprocess.run(["/usr/bin/git", "-C", str(root), *args], env=env,
                                  stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, timeout=10)
        except subprocess.TimeoutExpired as error:
            raise ValueError("B8 fixed Git source read timed out") from error
    size = fixed_git("cat-file", "-s", selector)
    if size.returncode or not re.fullmatch(rb"[0-9]{1,9}\n", size.stdout):
        raise ValueError("B8 exact-source blob size unavailable")
    expected_size = int(size.stdout)
    if not 0 < expected_size <= limit:
        raise ValueError("B8 exact-source blob exceeds bound")
    blob = fixed_git("show", selector)
    if blob.returncode or len(blob.stdout) != expected_size:
        raise ValueError("B8 exact-source blob differs")
    return blob.stdout


def _appledouble(raw: bytes, provenance: bytes) -> None:
    if len(raw) != 152 + len(provenance) or not 0 < len(provenance) <= 64:
        raise ValueError("B8 AppleDouble metadata length differs")
    magic, version, host, count = struct.unpack_from(">II16sH", raw)
    if (magic, version, host, count) != (0x00051607, 0x00020000, b"Mac OS X        ", 2):
        raise ValueError("B8 AppleDouble header differs")
    first = struct.unpack_from(">III", raw, 26)
    second = struct.unpack_from(">III", raw, 38)
    if first != (9, 50, len(raw) - 50) or second != (2, len(raw), 0):
        raise ValueError("B8 AppleDouble entry table differs")
    if (raw[50:84] != bytes(34) or raw[84:88] != b"ATTR" or raw[88:92] != bytes(4)
            or struct.unpack_from(">III", raw, 92) != (len(raw), 152, len(provenance))
            or raw[104:118] != bytes(14) or struct.unpack_from(">HII", raw, 118) != (1, 152, len(provenance))
            or raw[128:130] != bytes(2) or raw[130] != 21
            or raw[131:152] != b"com.apple.provenance\x00" or raw[152:] != provenance):
        raise ValueError("B8 AppleDouble provenance payload differs")
