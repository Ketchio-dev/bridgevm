"""Sealed private input assets for the no-3D installed Windows diagnostic."""
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import subprocess

PROFILE = "no-3d-installed-input-450s"
ASSETS = {"image", "vars", "binary", "firmware"}


def digest(path):
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW)
    with os.fdopen(fd, "rb") as stream:
        if not stat.S_ISREG(os.fstat(stream.fileno()).st_mode):
            raise ValueError("asset is not regular")
        result = hashlib.sha256()
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            result.update(block)
    return result.hexdigest()


def load(manifest, root):
    from guest_input_protocol import regular_bytes
    raw = regular_bytes(manifest, 16384)
    value = json.loads(raw)
    if (set(value) != {"schema", "purpose", "profile", "assets"}
            or value["schema"] != "bridgevm.guest-input-live.v1"
            or value["purpose"] != "diagnostic-only" or value["profile"] != PROFILE
            or not isinstance(value["assets"], dict) or set(value["assets"]) != ASSETS):
        raise ValueError("invalid diagnostic input manifest")
    paths, hashes, identities = {}, {}, set()
    for name, entry in value["assets"].items():
        if not isinstance(entry, dict) or set(entry) != {"path", "sha256"}:
            raise ValueError("invalid asset entry")
        if not isinstance(entry["path"], str) or not isinstance(entry["sha256"], str):
            raise ValueError("invalid asset types")
        path = Path(entry["path"])
        if not path.is_absolute() or str(path.resolve()) != entry["path"] or path.is_symlink():
            raise ValueError("asset path must be canonical and absolute")
        info = path.stat()
        identity = (info.st_dev, info.st_ino)
        if not stat.S_ISREG(info.st_mode) or identity in identities:
            raise ValueError("non-regular or aliased assets")
        identities.add(identity)
        if info.st_size == 0 or (name == "image" and info.st_size % 512):
            raise ValueError("empty or misaligned asset")
        if name == "vars" and info.st_size != 64 * 1024 * 1024:
            raise ValueError("vars must cover the native 64 MiB slot")
        if not re.fullmatch(r"[0-9a-f]{64}", entry["sha256"]):
            raise ValueError("invalid asset hash")
        if digest(path) != entry["sha256"]:
            raise ValueError("asset hash mismatch")
        paths[name], hashes[name] = path, entry["sha256"]
    pinned = root / "crates/bridgevm-hvf/firmware/edk2-aarch64-secure-code.fd"
    if paths["firmware"] != pinned.resolve():
        raise ValueError("firmware is not repository-pinned")
    return paths, hashes, hashlib.sha256(raw).hexdigest()


def clone_media(paths, hashes, work):
    work.mkdir(mode=0o700, parents=False, exist_ok=False)
    result = {}
    for name, filename in (("image", "image.raw"), ("vars", "vars.fd")):
        source, target = paths[name], work / filename
        if source.stat().st_dev != work.stat().st_dev:
            raise ValueError("APFS clones require same-volume sources")
        subprocess.run(["/bin/cp", "-c", str(source), str(target)], check=True, timeout=60)
        if source.stat().st_ino == target.stat().st_ino or digest(target) != hashes[name]:
            raise ValueError("clone identity or content mismatch")
        target.chmod(0o600)
        result[name] = target
    return result
