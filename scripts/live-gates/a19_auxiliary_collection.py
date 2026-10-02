"""Collect all declared interruption cases transactionally from retained artifacts."""
from __future__ import annotations

import os
from pathlib import Path

from a19_auxiliary_protocol import manifest, media, protocol, selected, stop
from a19_first_restore_collection import VM_ID, collect_first
from a19_interrupt_cases import ADDED_POINTS, CASE_FLAGS, case_count, validate_cases
from a19_interrupt_stop_points import CREATE_VM_ID, STAGED_RESTORE
from native_snapshot_export_json import load_json


def case_flags(prefix: str, observation: dict) -> dict:
    result = {prefix + field: observation[field] for field in
              ("interruption_stage", "helper_stop_verified", "staged_file_sync_order_verified",
               "old_selection_before_kill", "helper_killed_and_reaped", "stop_fd_log_sha256")}
    result.update({prefix + field: True for field in
                   ("selection_unchanged_after_kill", "retry_succeeded")})
    return result


def swap_case(output: Path, snapshot: dict) -> dict:
    root = output / "aux-swap"
    observed = stop(root, ADDED_POINTS["swap_"].name, output, "swap")
    before, after, retry = (selected(root, name) for name in ("preinterrupt", "postkill", "postretry"))
    result, result_hash = protocol(root / "retry.stdout", VM_ID)
    if (before != after or retry != media(snapshot) or result != snapshot or
            before["disk_sha256"] == snapshot["disk_sha256"]):
        raise ValueError("auxiliary swap changed the old pair or its retry was ineffective")
    value = case_flags("swap_", observed)
    for name, pair in (("preinterrupt", before), ("postkill", after), ("postretry", retry)):
        value.update({f"swap_{name}_{field}": pair[field] for field in ("disk_sha256", "vars_sha256")})
    value["swap_retry_result_sha256"] = result_hash
    return value


def create_case(output: Path) -> dict:
    root = output / "aux-create"
    observed = stop(root, ADDED_POINTS["create_"].name, output, "create")
    state, _ = load_json(root / "destination-state.private.json")
    expected = str(output.resolve() / "live/auxiliary/create/export.snapshot")
    if (not isinstance(state, dict) or set(state) != {
            "destination_path", "preinterrupt_exists", "postkill_exists"} or
            state["destination_path"] != expected or
            state["preinterrupt_exists"] is not False or state["postkill_exists"] is not False):
        raise ValueError("auxiliary create lacks affirmative fresh-destination evidence")
    if any(os.path.lexists(root / name) for name in
           ("preinterrupt-manifest.json", "postkill-manifest.json")):
        raise ValueError("auxiliary create must begin and remain an absent destination")
    source = selected(root, "source")
    if any(selected(root, name) != source for name in ("source-postkill", "source-postretry")):
        raise ValueError("auxiliary create changed its source pair")
    retry = selected(root, "postretry")
    result, result_hash = protocol(root / "retry.stdout", CREATE_VM_ID)
    retained, manifest_hash = load_json(root / "retry-manifest.json")
    verified = manifest(retained, CREATE_VM_ID)
    if result != verified or media(verified) != retry or retry != source:
        raise ValueError("auxiliary create retry does not match its source or manifest")
    value = case_flags("create_", observed)
    for name, pair in (("source", source), ("postretry", retry)):
        value.update({f"create_{name}_{field}": pair[field] for field in ("disk_sha256", "vars_sha256")})
    value.update({"create_preinterrupt_manifest_sha256": "absent",
                  "create_postkill_manifest_sha256": "absent",
                  "create_retry_result_sha256": result_hash,
                  "create_postretry_manifest_sha256": manifest_hash})
    return value


def collect(output: Path, receipt: dict) -> None:
    if receipt["schema_version"] != 2:
        raise ValueError("production collection requires a schema 2 receipt")
    candidate = dict(receipt)
    stop(output, STAGED_RESTORE.name, output, "first")
    collect_first(output, candidate, verify_cli_results=True)
    snapshot, _ = load_json(output / "snapshot-created-manifest.json")
    verified = manifest(snapshot, VM_ID)
    candidate.update(swap_case(output, verified))
    candidate.update(create_case(output))
    if case_count(candidate) != 3 or any(not candidate[prefix + flag]
            for prefix in ADDED_POINTS for flag in CASE_FLAGS):
        raise ValueError("production collection requires all three interruption cases")
    validate_cases({**candidate, "pass": True})
    receipt.update(candidate)
