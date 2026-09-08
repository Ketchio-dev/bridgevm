#!/usr/bin/env python3
"""Fail-closed provenance check for the PresentMon diagnostic binary.

PresentMon is a third-party guest-side frame-time capture tool needed by the
B6 glyph matrix (measuring "frame time within 10% of baseline" per cell). It
is not a security-relevant component and never ships in a product image; it
is staged into a guest only for a live diagnostic capture. Provenance still
matters: an unpinned "run whatever .exe is on disk" step would let any file
named PresentMon-2.5.1-x64.exe execute as SYSTEM inside the guest.

The pin is the exact digest GitHub's release API reports for the asset,
recorded here from `gh api repos/GameTechDev/PresentMon/releases/tags/v2.5.1`
on 2026-09-08. A version bump requires updating PINNED explicitly; this
script never fetches or trusts anything at runtime.
"""
from __future__ import annotations

import argparse
import hashlib
import sys
from pathlib import Path

PINNED = {
    "PresentMon-2.5.1-x64.exe": {
        "sha256": "9bec3083069f58f911e6a512f4806db51a27bd096103087bc1d05ef54c80a191",
        "bytes": 956768,
        "source_url": "https://github.com/GameTechDev/PresentMon/releases/download/v2.5.1/PresentMon-2.5.1-x64.exe",
    },
}


def digest(path: Path) -> str:
    hasher = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            hasher.update(block)
    return hasher.hexdigest()


def verify(path: Path) -> str:
    """Return the pinned sha256 for `path`, or raise ValueError."""
    pin = PINNED.get(path.name)
    if pin is None:
        raise ValueError(f"{path.name} is not a pinned PresentMon release asset")
    if not path.is_file() or path.is_symlink():
        raise ValueError(f"{path} is missing or not a regular file")
    size = path.stat().st_size
    if size != pin["bytes"]:
        raise ValueError(f"{path.name} size {size} does not match pinned {pin['bytes']}")
    actual = digest(path)
    if actual != pin["sha256"]:
        raise ValueError(f"{path.name} sha256 {actual} does not match pinned {pin['sha256']}")
    return actual


def self_test() -> None:
    import tempfile

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        wrong_name = root / "not-presentmon.exe"
        wrong_name.write_bytes(b"x")
        try:
            verify(wrong_name)
            raise SystemExit("unpinned filename must be refused")
        except ValueError:
            pass

        wrong_size = root / "PresentMon-2.5.1-x64.exe"
        wrong_size.write_bytes(b"short")
        try:
            verify(wrong_size)
            raise SystemExit("wrong size must be refused")
        except ValueError:
            pass

        wrong_bytes = root / "PresentMon-2.5.1-x64.exe"
        wrong_bytes.write_bytes(b"\x00" * PINNED["PresentMon-2.5.1-x64.exe"]["bytes"])
        try:
            verify(wrong_bytes)
            raise SystemExit("wrong content must be refused by hash even with correct size")
        except ValueError:
            pass

        symlink_target = root / "real.exe"
        symlink_target.write_bytes(b"\x00" * PINNED["PresentMon-2.5.1-x64.exe"]["bytes"])
        symlink_path = root / "PresentMon-2.5.1-x64.exe"
        symlink_path.unlink()
        symlink_path.symlink_to(symlink_target)
        try:
            verify(symlink_path)
            raise SystemExit("symlink must be refused")
        except ValueError:
            pass

    print("presentmon provenance self-test: PASS")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("path", nargs="?", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return 0
    if args.path is None:
        parser.error("path is required unless --self-test is given")
    try:
        actual = verify(args.path)
    except ValueError as error:
        print(f"FAIL: {error}", file=sys.stderr)
        return 1
    print(f"presentmon provenance verified sha256={actual}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
