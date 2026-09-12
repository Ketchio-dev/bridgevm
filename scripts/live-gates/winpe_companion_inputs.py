"""Sealed inputs and independent media for a no-3D WinPE diagnostic."""
import os
from pathlib import Path
import re
import subprocess

from b6_cell_inputs import file_hash, small_bytes

ASSETS = ("image", "vars", "injector", "binary", "firmware")
MEDIA = ("image", "vars", "injector")
FIXED = {"schema": "bridgevm.winpe-companions.v1", "purpose": "diagnostic-only",
         "profile": "no-3d-winpe-300s", "claim_eligible": "false"}
FIRMWARE = "crates/bridgevm-hvf/firmware/edk2-aarch64-secure-code.fd"
COMPANIONS = ("bvagent.ps1", "bvagent-input.ps1", "bvagent-unicode-input.cs",
              "bvagent-key-input.cs", "bvagent-pointer-input.cs")


def verify(records):
    identities = set()
    for key, (path, expected) in records.items():
        if file_hash(path) != expected:
            raise ValueError("changed input: " + key)
        info = path.stat()
        identity = info.st_dev, info.st_ino
        if identity in identities:
            raise ValueError("aliased inputs")
        identities.add(identity)
        if key == "vars" and info.st_size != 64 * 1024 * 1024:
            raise ValueError("invalid vars size")
        if key in ("image", "injector") and (info.st_size == 0 or info.st_size % 512):
            raise ValueError("invalid disk size")


def load(manifest, repo, binary):
    metadata, records = {}, {}
    for line in small_bytes(manifest, 16384).decode("utf-8").splitlines():
        fields = line.split("\t")
        key = fields[0]
        if len(fields) == 2 and key in FIXED and key not in metadata:
            metadata[key] = fields[1]
        elif len(fields) == 3 and key in ASSETS and key not in records:
            raw, digest = fields[1:]
            if not raw.startswith("/") or os.path.normpath(raw) != raw or any(c in raw for c in "\0\r\n"):
                raise ValueError("invalid asset path")
            if not re.fullmatch(r"[0-9a-f]{64}", digest):
                raise ValueError("invalid asset digest")
            path = binary if key == "binary" else repo / FIRMWARE if key == "firmware" else Path(raw)
            records[key] = (path, digest)
        else:
            raise ValueError("unknown, duplicate or malformed row")
    if metadata != FIXED or set(records) != set(ASSETS):
        raise ValueError("incomplete fixed diagnostic contract")
    verify(records)
    return records


def clone(records, work, copy=subprocess.run):
    work.mkdir(mode=0o700, exist_ok=False)
    if any(records[key][0].stat().st_dev != work.stat().st_dev for key in MEDIA):
        raise ValueError("stage external media internally before cloning")
    clones = {}
    identities = {(path.stat().st_dev, path.stat().st_ino) for path, _ in records.values()}
    for key in MEDIA:
        source, digest = records[key]
        target = work / (key + (".fd" if key == "vars" else ".raw"))
        copy(["cp", "-c", str(source), str(target)], check=True)
        info = target.stat()
        identity = info.st_dev, info.st_ino
        if target.is_symlink() or identity in identities or file_hash(target) != digest:
            raise ValueError("invalid independent clone")
        identities.add(identity)
        target.chmod(0o600)
        clones[key] = target
    return clones
