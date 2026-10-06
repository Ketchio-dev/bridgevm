"""Retained first-restore evidence; its four guest boots stay a separate proof."""
from __future__ import annotations

import hashlib
import re
from pathlib import Path

from a19_interrupt_stop_points import STAGED_RESTORE
from a19_interrupted_restore_seal import read_bounded_regular
from native_snapshot_export_json import load_json
from native_snapshot_restore_artifacts import digest, regular
from native_snapshot_export_evidence import load_evidence

SHA256 = re.compile(r"[0-9a-f]{64}\Z")
VM_ID = "a19-native-cli-live"


def read_hash(path: Path) -> str:
    value = read_bounded_regular(path, 128).decode("ascii").strip()
    if not SHA256.fullmatch(value):
        raise ValueError("retained hash is invalid")
    return value


def file_hash(path: Path) -> str:
    return digest(path)


def parse_stop(path: Path, raw: Path, expected_point: str = STAGED_RESTORE.name) -> dict:
    regular(path)
    value, _ = load_json(path)
    expected = {"interruption_stage", "helper_stop_verified", "staged_file_sync_order_verified",
                "old_selection_before_kill", "helper_killed_and_reaped", "stop_fd_log_sha256"}
    if not isinstance(value, dict) or set(value) != expected:
        raise ValueError("stop observation field set is invalid")
    if value["interruption_stage"] != expected_point or any(
        value[field] is not True for field in expected - {"interruption_stage", "stop_fd_log_sha256"}
    ):
        raise ValueError("stop observation does not prove the declared point")
    if value["stop_fd_log_sha256"] != hashlib.sha256(read_bounded_regular(raw, 65_536)).hexdigest():
        raise ValueError("retained FD observation changed")
    return value


def export_fields(output: Path) -> dict:
    return load_evidence(output / "export-evidence.json", VM_ID)


def snapshot_hashes(path: Path) -> tuple[str, str]:
    regular(path)
    value, _ = load_json(path)
    if (not isinstance(value, dict) or set(value) != {"format_version", "vm_id", "disk_bytes",
        "disk_sha256", "vars_bytes", "vars_sha256"} or value["format_version"] != 1 or
        value["vm_id"] != VM_ID or any(type(value[key]) is not int or value[key] <= 0
        for key in ("disk_bytes", "vars_bytes")) or any(not isinstance(value[key], str) or
        not SHA256.fullmatch(value[key]) for key in ("disk_sha256", "vars_sha256"))):
        raise ValueError("created snapshot manifest is invalid")
    return value["disk_sha256"], value["vars_sha256"]


def cli_result(output: Path, filename: str, command: str) -> str:
    value, result_hash = load_json(output / filename)
    library = output / "live/library"
    snapshot = library / VM_ID / "bundle.vmbridge/metadata/snapshots/latest.snapshot"
    expected = {"schema": "bridgevm.app-snapshot.v1", "command": command, "vmID": VM_ID,
                "libraryPath": str(library), "snapshotPath": str(snapshot), "complete": True}
    if value != expected or type(value["complete"]) is not bool:
        raise ValueError("native CLI result does not bind the requested operation and snapshot")
    return result_hash


def collect_first(output: Path, receipt: dict, verify_cli_results: bool = False) -> None:
    stop = parse_stop(output / "interrupt-observation.json",
                      output / "interrupt-helper-fd.private.log")
    receipt.update(stop)
    postkill = export_fields(output / "postkill-export")
    postretry = export_fields(output / "postretry-export")
    snapshot_disk, snapshot_vars = snapshot_hashes(output / "snapshot-created-manifest.json")
    create_hash = (cli_result(output, "create.json", "create") if verify_cli_results
                   else file_hash(output / "create.json"))
    retry_hash = (cli_result(output, "restore-retry.json", "restore") if verify_cli_results
                  else file_hash(output / "restore-retry.json"))
    receipt.update({
        "snapshot_create_result_sha256": create_hash,
        "snapshot_restore_retry_result_sha256": retry_hash,
        "snapshot_disk_sha256": snapshot_disk, "snapshot_vars_sha256": snapshot_vars,
        "preinterrupt_disk_sha256": read_hash(output / "pre-interrupt-disk.sha256"),
        "preinterrupt_vars_sha256": read_hash(output / "pre-interrupt-vars.sha256"),
        "postkill_disk_sha256": read_hash(output / "postkill-disk.sha256"),
        "postkill_vars_sha256": read_hash(output / "postkill-vars.sha256"),
        "postretry_disk_sha256": postretry["disk_sha256"],
        "postretry_vars_sha256": postretry["vars_sha256"],
        "postkill_export_result_sha256": postkill["result_sha256"],
        "postkill_export_manifest_sha256": postkill["manifest_sha256"],
        "postretry_export_result_sha256": postretry["result_sha256"],
        "postretry_export_manifest_sha256": postretry["manifest_sha256"],
        "original_marker_sha256": file_hash(output / "phase1-original/marker-after.txt"),
        "clobber_marker_sha256": file_hash(output / "phase3-clobber/marker-after.txt"),
        "postkill_marker_sha256": file_hash(output / "phase5-postkill/marker-before.txt"),
        "restored_marker_sha256": file_hash(output / "phase7-restored/marker-before.txt"),
    })
    if (postkill["disk_sha256"], postkill["vars_sha256"]) != (
        receipt["postkill_disk_sha256"], receipt["postkill_vars_sha256"]):
        raise ValueError("postkill export differs from independent hash")
    receipt["postkill_pair_unchanged"] = (
        receipt["preinterrupt_disk_sha256"], receipt["preinterrupt_vars_sha256"]
    ) == (receipt["postkill_disk_sha256"], receipt["postkill_vars_sha256"])
    receipt["postretry_original_restored"] = (
        receipt["postretry_disk_sha256"], receipt["postretry_vars_sha256"],
        receipt["restored_marker_sha256"],
    ) == (snapshot_disk, snapshot_vars, receipt["original_marker_sha256"])
    if not (receipt["postkill_pair_unchanged"] and receipt["postretry_original_restored"] and
            receipt["postkill_marker_sha256"] == receipt["clobber_marker_sha256"] and
            receipt["original_marker_sha256"] != receipt["clobber_marker_sha256"]):
        raise ValueError("interrupted or retried guest pair identity is wrong")
