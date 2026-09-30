"""Declared T22 stop points and the old selection each must leave in place.

Each point stops a snapshot helper while it reads its staged disk, after the
staged files were synced and before publication, and only while the selection
it declares is the one that held before launch.
"""
from __future__ import annotations

from dataclasses import dataclass
import hashlib
import os
from pathlib import Path


@dataclass(frozen=True)
class StopPoint:
    name: str
    operation: str
    selection: str


STAGED_RESTORE = StopPoint("staged-disk-verify-read", "restore", "initial")
SWAP_RESTORE = StopPoint("swap-staged-disk-verify-read", "restore", "generation")
CREATE_EXPORT = StopPoint("create-staged-disk-hash-read", "create", "snapshot")
STOP_POINTS = {point.name: point for point in (STAGED_RESTORE, SWAP_RESTORE, CREATE_EXPORT)}
CREATE_VM_ID = "a19-interrupted-create"


def stable_root(disk: Path, vars: Path) -> Path:
    relative_vars = Path(os.path.relpath(vars, disk.parent))
    digest = hashlib.sha256()
    for value in (disk.name, str(relative_vars)):
        data = os.fsencode(value)
        digest.update(len(data).to_bytes(8, "little"))
        digest.update(data)
    return disk.parent / (".bridgevm-pair-v2-" + digest.hexdigest())


def regular(path: Path) -> bool:
    return path.is_file() and not path.is_symlink()


def real_dir(path: Path) -> bool:
    return path.is_dir() and not path.is_symlink()


def staging(point: StopPoint, root: Path, snapshot: Path) -> Path:
    if point.operation == "create":
        return snapshot.parent / f".{snapshot.name}.staging"
    return root / "staging"


def staged_ready(point: StopPoint, stage: Path) -> bool:
    if not real_dir(stage):
        return False
    if point.operation == "create":
        # Creation hashes the disk after syncing both copies and before the manifest.
        return (regular(stage / "disk.raw") and regular(stage / "vars.fd") and
                not os.path.lexists(stage / "manifest.json"))
    return all(regular(stage / name) for name in ("disk.raw", "vars.fd", "manifest.json"))


def identity(paths: list[Path]) -> str:
    digest = hashlib.sha256()
    for path in paths:
        status = os.lstat(path)
        digest.update(f"{status.st_dev}:{status.st_ino}:{status.st_size}:{status.st_mtime_ns}\n".encode())
    return digest.hexdigest()


def selection(point: StopPoint, root: Path, snapshot: Path) -> str | None:
    """What is selected now, or None when the point's declared selection is absent."""
    current = root / "current"
    if point.selection == "initial":
        return None if os.path.lexists(current) else "initial"
    if point.selection == "generation":
        members = [current / "disk.raw", current / "vars.fd"]
        if not real_dir(current) or not all(regular(path) for path in members):
            return None
        return identity([current, *members])
    if not os.path.lexists(snapshot):
        return "absent"
    manifest = snapshot / "manifest.json"
    if not real_dir(snapshot) or not regular(manifest):
        return None
    return identity([snapshot, manifest])


def helper_command(point: StopPoint, helper: Path, snapshot: Path, disk: Path,
                   vars: Path, root: Path) -> list[str]:
    if point.operation == "create":
        # Creation's quota covers the selected pair, which a generation replaces.
        current = root / "current"
        selected = [current / "disk.raw", current / "vars.fd"] if real_dir(current) else [disk, vars]
        quota = sum(path.stat().st_size for path in selected)
        return [str(helper), "create", str(disk), str(vars), str(snapshot), CREATE_VM_ID, str(quota)]
    return [str(helper), "restore", str(snapshot), str(disk), str(vars)]
