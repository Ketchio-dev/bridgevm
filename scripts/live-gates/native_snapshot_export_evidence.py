#!/usr/bin/env python3
"""Verify a native app snapshot export before a live boot consumes it."""
from __future__ import annotations

import hashlib
import json
import re
import stat
import sys
from pathlib import Path

SHA256 = re.compile(r"^[0-9a-f]{64}$")
RESULT_FIELDS = {"schema", "command", "vmID", "libraryPath", "snapshotPath", "complete"}
MANIFEST_FIELDS = {
    "format_version", "vm_id", "disk_bytes", "disk_sha256", "vars_bytes", "vars_sha256",
}
EVIDENCE_FIELDS = {
    "schema", "vm_id", "result_sha256", "manifest_sha256", "disk_sha256", "vars_sha256",
}


def digest(path: Path) -> str:
    value = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(8 * 1024 * 1024), b""):
            value.update(chunk)
    return value.hexdigest()


def regular(path: Path) -> None:
    mode = path.lstat().st_mode
    if not stat.S_ISREG(mode) or path.is_symlink():
        raise ValueError(f"{path.name} is not a regular non-symlink file")


def verify(result_path: Path, export: Path, vm_id: str, library: Path) -> dict[str, str]:
    regular(result_path)
    if not export.is_absolute() or export.is_symlink() or not export.is_dir():
        raise ValueError("export directory is not an absolute non-symlink directory")
    if {entry.name for entry in export.iterdir()} != {"disk.raw", "vars.fd", "manifest.json"}:
        raise ValueError("export directory has an unexpected file set")
    disk, variables, manifest_path = (export / "disk.raw", export / "vars.fd", export / "manifest.json")
    for path in (disk, variables, manifest_path):
        regular(path)
    result = json.loads(result_path.read_text(encoding="utf-8"))
    if not isinstance(result, dict) or set(result) != RESULT_FIELDS:
        raise ValueError("app export result has an unexpected field set")
    expected = {
        "schema": "bridgevm.app-snapshot.v1", "command": "export", "vmID": vm_id,
        "libraryPath": str(library), "snapshotPath": str(export), "complete": True,
    }
    if result != expected:
        raise ValueError("app export result does not bind the requested VM, library and output")
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    if not isinstance(manifest, dict) or set(manifest) != MANIFEST_FIELDS:
        raise ValueError("snapshot export manifest has an unexpected field set")
    if manifest["format_version"] != 1 or manifest["vm_id"] != vm_id:
        raise ValueError("snapshot export manifest identity is invalid")
    disk_hash, vars_hash = digest(disk), digest(variables)
    for path, size_key, hash_key, actual_hash in (
        (disk, "disk_bytes", "disk_sha256", disk_hash),
        (variables, "vars_bytes", "vars_sha256", vars_hash),
    ):
        size = manifest[size_key]
        if not isinstance(size, int) or isinstance(size, bool) or size < 0 or size != path.stat().st_size:
            raise ValueError(f"snapshot export {size_key} is invalid")
        if not isinstance(manifest[hash_key], str) or not SHA256.fullmatch(manifest[hash_key]):
            raise ValueError(f"snapshot export {hash_key} is invalid")
        if manifest[hash_key] != actual_hash:
            raise ValueError(f"snapshot export {path.name} does not match its manifest")
    return {
        "schema": "bridgevm.native-snapshot-export-evidence.v1", "vm_id": vm_id,
        "result_sha256": digest(result_path), "manifest_sha256": digest(manifest_path),
        "disk_sha256": disk_hash, "vars_sha256": vars_hash,
    }


def load_evidence(path: Path, vm_id: str) -> dict[str, str]:
    regular(path)
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict) or set(value) != EVIDENCE_FIELDS:
        raise ValueError("snapshot export evidence has an unexpected field set")
    if value["schema"] != "bridgevm.native-snapshot-export-evidence.v1" or value["vm_id"] != vm_id:
        raise ValueError("snapshot export evidence identity is invalid")
    for field in EVIDENCE_FIELDS - {"schema", "vm_id"}:
        if not isinstance(value[field], str) or not SHA256.fullmatch(value[field]):
            raise ValueError(f"snapshot export evidence {field} is invalid")
    return value


def receipt_fields(value: dict[str, str]) -> dict[str, str]:
    return {
        "snapshot_export_result_sha256": value["result_sha256"],
        "snapshot_export_manifest_sha256": value["manifest_sha256"],
        "exported_disk_sha256": value["disk_sha256"],
        "exported_vars_sha256": value["vars_sha256"],
    }


def main() -> int:
    if len(sys.argv) != 5:
        print("usage: native_snapshot_export_evidence.py RESULT EXPORT VM_ID LIBRARY", file=sys.stderr)
        return 2
    try:
        value = verify(Path(sys.argv[1]), Path(sys.argv[2]), sys.argv[3], Path(sys.argv[4]))
    except (OSError, UnicodeError, ValueError, json.JSONDecodeError) as error:
        print(f"FAIL: snapshot export evidence: {error}", file=sys.stderr)
        return 1
    json.dump(value, sys.stdout, sort_keys=True, separators=(",", ":")); print()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
