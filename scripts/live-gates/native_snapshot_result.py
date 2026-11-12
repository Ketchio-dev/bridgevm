#!/usr/bin/env python3
"""Bind native snapshot results to a direct, non-symlink child of the VM snapshot folder."""
import re
import sys
from pathlib import Path
from native_snapshot_export_json import load_json


def snapshot_path(value, bundle: Path) -> Path:
    if not isinstance(value, str):
        raise ValueError("native snapshot path is missing")
    path = Path(value)
    parent = bundle / "metadata/snapshots"
    if (not path.is_absolute() or path.parent != parent or str(path) != value or
        not re.fullmatch(r"(?:latest|[0-9]{8}-[0-9]{6})\.snapshot", path.name) or
        path.is_symlink() or parent.resolve() != bundle.resolve() / "metadata/snapshots" or
        path.resolve().parent != parent.resolve()):
        raise ValueError("native snapshot path is not a safe direct child")
    return path


def result_path(document: Path, bundle: Path, vm_id: str, command: str, expected=None) -> tuple[Path, str]:
    value, digest = load_json(document)
    if not isinstance(value, dict):
        raise ValueError("native snapshot result is not an object")
    path = snapshot_path(value.get("snapshotPath"), bundle)
    fields = dict(schema="bridgevm.app-snapshot.v1", command=command, vmID=vm_id,
                  libraryPath=str(bundle.parent.parent), snapshotPath=str(path), complete=True)
    if value != fields or type(value["complete"]) is not bool or (expected is not None and str(path) != str(expected)):
        raise ValueError("native CLI result does not bind the requested operation and snapshot")
    return path, digest


if __name__ == '__main__':
    try:
        print(result_path(Path(sys.argv[1]), Path(sys.argv[2]), sys.argv[3], sys.argv[4],
                          sys.argv[5] if len(sys.argv) > 5 else None)[0])
    except (ValueError, OSError) as error:
        raise SystemExit(str(error))
