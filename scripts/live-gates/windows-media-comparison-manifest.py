#!/usr/bin/env python3
"""Authenticate diagnostic media A/B inputs; never a capability gate."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import sys

FIXED = {
    "schema": "bridgevm.windows-media-comparison.v1",
    "purpose": "diagnostic-only",
    "claim_eligible": "false",
    "order": "original,reinjected",
    "sample_count": "2",
    "profile": "windows-closure-proof-v1",
}
ASSETS = ("original", "reinjected", "vars", "binary", "firmware",
          "viogpu_dir", "virglrenderer", "moltenvk")
FIRMWARE = "crates/bridgevm-hvf/firmware/edk2-aarch64-secure-code.fd"


def seal(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def tree_seal(path):
    files = sorted(path.rglob("*"), key=lambda item: item.relative_to(path).as_posix())
    if not files or any(item.is_symlink() for item in files):
        raise ValueError("empty or symlink-containing driver tree")
    rows = []
    for item in files:
        if item.is_file():
            name = item.relative_to(path).as_posix()
            if any(char in name for char in "\r\n\\"):
                raise ValueError("unsafe driver filename")
            rows.append(f"{seal(item)}  ./{name}\n")
        elif not item.is_dir():
            raise ValueError("non-regular driver entry")
    if not rows:
        raise ValueError("driver tree has no files")
    return hashlib.sha256("".join(rows).encode()).hexdigest()


def verify(manifest, repo, sealed_binary):
    if manifest.is_symlink() or not manifest.is_file() or not 0 < manifest.stat().st_size <= 16384:
        raise ValueError("invalid manifest file")
    metadata, assets = {}, {}
    for line in manifest.read_text().splitlines():
        fields = line.split("\t")
        key = fields[0]
        if len(fields) == 2 and key in FIXED and key not in metadata:
            metadata[key] = fields[1]
        elif len(fields) == 3 and key in ASSETS and key not in assets:
            raw, expected = fields[1:]
            if not re.fullmatch(r"[0-9a-f]{64}", expected):
                raise ValueError("invalid digest")
            if not raw.startswith("/") or os.path.normpath(raw) != raw:
                raise ValueError("asset path must be normalized and absolute")
            assets[key] = (Path(raw), expected)
        else:
            raise ValueError("unknown, duplicate or malformed row")
    if metadata != FIXED or set(assets) != set(ASSETS):
        raise ValueError("fixed diagnostic contract mismatch")
    verified, identities = {}, set()
    for key in ASSETS:
        path, expected = assets[key]
        if key == "binary":
            path = sealed_binary
        if key == "firmware":
            path = repo / FIRMWARE
        if path.is_symlink() or not (path.is_dir() if key == "viogpu_dir" else path.is_file()):
            raise ValueError(f"unsafe or missing {key}")
        identity = (path.stat().st_dev, path.stat().st_ino)
        if identity in identities:
            raise ValueError("input assets alias")
        identities.add(identity)
        actual = tree_seal(path) if key == "viogpu_dir" else seal(path)
        if actual != expected:
            raise ValueError(f"changed {key}")
        verified[key] = {"path": str(path), "sha256": actual}
    if verified["original"]["sha256"] == verified["reinjected"]["sha256"]:
        raise ValueError("media contents must differ for this comparison")
    return {"valid": True, "claim_eligible": False, "metadata": metadata, "assets": verified}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", required=True, type=Path)
    parser.add_argument("--repo", required=True, type=Path)
    parser.add_argument("--sealed-binary", required=True, type=Path)
    args = parser.parse_args()
    try:
        result = verify(args.manifest, args.repo, args.sealed_binary)
    except (OSError, UnicodeError, ValueError) as error:
        print(json.dumps({"valid": False, "claim_eligible": False, "detail": str(error)}))
        return 1
    print(json.dumps(result, sort_keys=True))
    return 0


if __name__ == "__main__":
    sys.exit(main())
