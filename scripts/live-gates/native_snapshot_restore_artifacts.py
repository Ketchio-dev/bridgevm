"""App-bundle identity and cloning helpers for the native snapshot tier."""
from __future__ import annotations

import hashlib
import os
from pathlib import Path
import re
import stat
import subprocess

RELATIONS = {
    "app_cli": "Contents/Resources/target/release/bridgevm",
    "app_executable": "Contents/MacOS/BridgeVMControl",
    "snapshot_helper": "Contents/Resources/target/release/examples/snapshot_pair_cli",
    "binary": "Contents/Resources/target/release/examples/hvf_gic_boot_probe",
}
SHA256 = re.compile(r"^[0-9a-f]{64}$")


def regular(path: Path) -> None:
    if not stat.S_ISREG(path.lstat().st_mode):
        raise ValueError("artifact must be a non-symlink regular file")


def digest(path: Path) -> str:
    regular(path)
    output = subprocess.check_output(["openssl", "dgst", "-sha256", "-r", str(path)], text=True)
    value = output.split()[0]
    if not SHA256.fullmatch(value):
        raise ValueError("invalid artifact hash")
    return value


def tree_hash(root: Path) -> str:
    if not root.is_dir() or root.is_symlink():
        raise ValueError("app bundle must be a non-symlink directory")
    records: list[bytes] = []
    for path in sorted(root.rglob("*"), key=lambda item: item.relative_to(root).as_posix()):
        relative, mode = path.relative_to(root).as_posix(), path.lstat().st_mode
        if stat.S_ISLNK(mode):
            target = os.readlink(path)
            if os.path.isabs(target) or root.resolve() not in path.resolve().parents:
                raise ValueError("app bundle symlink escapes its root")
            records.append(f"L\t{relative}\t{target}\n".encode())
        elif stat.S_ISREG(mode):
            records.append(f"F\t{relative}\t{'1' if mode & 0o111 else '0'}\t{digest(path)}\n".encode())
        elif stat.S_ISDIR(mode):
            records.append(f"D\t{relative}\n".encode())
        else:
            raise ValueError("app bundle contains an unsupported entry")
    return hashlib.sha256(b"".join(records)).hexdigest()


def clone_app(source: Path, destination: Path) -> None:
    subprocess.run(["cp", "-cR", str(source), str(destination)], check=True)


def authenticate_app(rows: dict[str, list[str]], app: Path) -> None:
    if tree_hash(app) != rows["app_bundle"][1]:
        raise ValueError("app bundle hash mismatch")
    for key, relative in RELATIONS.items():
        if digest(app / relative) != rows[key][1]:
            raise ValueError(f"{key} hash mismatch")
