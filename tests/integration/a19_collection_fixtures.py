"""Tiny retained host artifacts for production T22 collection contracts."""
from __future__ import annotations

import hashlib
import json
from pathlib import Path

from a19_interrupt_stop_points import CREATE_VM_ID, SWAP_RESTORE, CREATE_EXPORT, stable_root

VM_ID = "a19-native-cli-live"


def put(root: Path, name: str, data: str | bytes) -> None:
    path = root / name
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data.encode() if isinstance(data, str) else data)


def hash_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def pair(disk: bytes, variables: bytes) -> dict:
    return {"disk_bytes": len(disk), "disk_sha256": hash_bytes(disk),
            "vars_bytes": len(variables), "vars_sha256": hash_bytes(variables)}


def manifest(value: dict, vm_id: str = VM_ID) -> dict:
    return {"format_version": 1, "vm_id": vm_id, **value}


def lines(value: dict) -> str:
    return "".join(f"{key} {item}\n" for key, item in value.items())


def selected(root: Path, name: str, value: dict) -> None:
    put(root, name + "-digest.txt", lines(value))
    put(root, name + "-host-digest.txt", lines(value))


def stop(root: Path, point: str, staged: Path, pid: int, selection: str) -> None:
    raw = f"p{pid}\nf3\nar\nn{staged}\n"
    put(root, "interrupt-helper-fd.private.log", raw)
    put(root, "interrupt-helper-context.private.json", json.dumps({
        "helper_pid": pid, "staged_disk_path": str(staged),
        "selection_before": selection, "selection_after": selection}))
    put(root, "interrupt-observation.json", json.dumps({
        "interruption_stage": point, "helper_stop_verified": True,
        "staged_file_sync_order_verified": True, "old_selection_before_kill": True,
        "helper_killed_and_reaped": True, "stop_fd_log_sha256": hash_bytes(raw.encode())}))


def first_case(output: Path) -> dict:
    original = pair(b"original disk", b"original vars")
    put(output, "snapshot-created-manifest.json", json.dumps(manifest(original)))
    library = output / "live/library"
    for filename, command in (("create.json", "create"), ("restore-retry.json", "restore")):
        put(output, filename, json.dumps({"schema": "bridgevm.app-snapshot.v1", "command": command,
            "vmID": VM_ID, "libraryPath": str(library), "complete": True,
            "snapshotPath": str(library / VM_ID / "bundle.vmbridge/metadata/snapshots/latest.snapshot")}))
    for name, content in {
            "phase1-original/marker-after.txt": "ORIGINAL\n",
            "phase3-clobber/marker-after.txt": "CLOBBER\n",
            "phase5-postkill/marker-before.txt": "CLOBBER\n",
            "phase7-restored/marker-before.txt": "ORIGINAL\n"}.items():
        put(output, name, content)
    for name in ("pre-interrupt", "postkill"):
        for key in ("disk", "vars"):
            put(output, f"{name}-{key}.sha256", original[key + "_sha256"] + "\n")
    for folder in ("postkill-export", "postretry-export"):
        put(output / folder, "export-evidence.json", json.dumps({
            "schema": "bridgevm.native-snapshot-export-evidence.v1", "vm_id": VM_ID,
            "result_sha256": "a" * 64, "manifest_sha256": "b" * 64,
            "disk_sha256": original["disk_sha256"], "vars_sha256": original["vars_sha256"]}))
    bundle = output.resolve() / "live/library/a19-native-cli-live/bundle.vmbridge"
    first_stage = stable_root(bundle / "disks/hvf-target.raw", bundle / "metadata/hvf-vars.fd") / "staging/disk.raw"
    stop(output, "staged-disk-verify-read", first_stage, 123, "initial")
    return original


def added_cases(output: Path, original: dict) -> None:
    old = pair(b"clobbered disk", b"clobbered vars")
    swap, create = output / "aux-swap", output / "aux-create"
    work = output.resolve() / "live/auxiliary"
    swap_disk, swap_vars = work / "swap/disk.raw", work / "swap/vars.fd"
    stop(swap, SWAP_RESTORE.name, stable_root(swap_disk, swap_vars) / "staging/disk.raw", 124, "d" * 64)
    for name, value in (("preinterrupt", old), ("postkill", old), ("postretry", original)):
        selected(swap, name, value)
    put(swap, "retry.stdout", lines(manifest(original)))
    stop(create, CREATE_EXPORT.name, work / "create/.export.snapshot.staging/disk.raw", 125, "absent")
    put(create, "destination-state.private.json", json.dumps({"destination_path": str(work / "create/export.snapshot"),
        "preinterrupt_exists": False, "postkill_exists": False}))
    for name in ("source", "source-postkill", "source-postretry", "postretry"):
        selected(create, name, old)
    put(create, "retry.stdout", lines(manifest(old, CREATE_VM_ID)))
    put(create, "retry-manifest.json", json.dumps(manifest(old, CREATE_VM_ID)))
