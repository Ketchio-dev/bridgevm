#!/usr/bin/env python3
"""Fail closed unless each audio gate source is an immutable regular file."""
import hashlib
import os
import stat
import sys
import tempfile
from pathlib import Path


def seal(path):
    flags = os.O_RDONLY | os.O_CLOEXEC | os.O_NOFOLLOW | os.O_NONBLOCK
    fd = os.open(path, flags)
    try:
        metadata = os.fstat(fd)
        if not stat.S_ISREG(metadata.st_mode):
            raise ValueError("source is not a regular file")
        if metadata.st_mode & 0o222:
            raise ValueError("source has a write bit")
        if metadata.st_size == 0:
            raise ValueError("source is empty")
        digest = hashlib.sha256()
        while chunk := os.read(fd, 1024 * 1024):
            digest.update(chunk)
        return digest.hexdigest()
    finally:
        os.close(fd)


def self_test():
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        source = root / "source"
        source.write_bytes(b"sealed")
        source.chmod(0o400)
        assert seal(source) == hashlib.sha256(b"sealed").hexdigest()
        rejected = []
        for candidate in (root / "writable", root / "empty", root / "link"):
            if candidate.name == "writable":
                candidate.write_bytes(b"mutable")
            elif candidate.name == "empty":
                candidate.touch(mode=0o400)
            else:
                candidate.symlink_to(source)
            try:
                seal(candidate)
            except (OSError, ValueError):
                rejected.append(candidate.name)
        assert rejected == ["writable", "empty", "link"]
    print("immutable audio source seal: PASS")


def main():
    if sys.argv[1:] == ["--self-test"]:
        self_test()
        return
    if len(sys.argv) != 3:
        raise SystemExit("usage: seal-immutable-audio-sources.py IMAGE VARS")
    try:
        hashes = [seal(path) for path in sys.argv[1:]]
    except (OSError, ValueError) as error:
        raise SystemExit(f"audio source validation failed: {error}") from error
    print(*hashes, sep="\t")


if __name__ == "__main__":
    main()
