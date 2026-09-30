"""Separately counted T22 interruption cases at declared stop points.

The top-level stop fields hold the first-restore case, which every pass needs.
Schema 2 adds flat fields for each other declared point, because the public
redaction allowlist is flat; a present case counts once with its own flags and hashes.
"""
from __future__ import annotations

import a19_interrupt_case_hashes as hashes
from a19_interrupt_stop_points import CREATE_EXPORT, STAGED_RESTORE, SWAP_RESTORE

ADDED_POINTS = {"swap_": SWAP_RESTORE, "create_": CREATE_EXPORT}
CASE_FLAGS = ("helper_stop_verified", "staged_file_sync_order_verified",
              "old_selection_before_kill", "helper_killed_and_reaped",
              "selection_unchanged_after_kill", "retry_succeeded")
ADDED_FIELDS = frozenset(prefix + field for prefix in ADDED_POINTS
                         for field in ("interruption_stage", *CASE_FLAGS)) | hashes.PROOF_FIELDS


def case_fields(value: dict) -> frozenset[str]:
    version = value.get("schema_version")
    return ADDED_FIELDS if type(version) is int and version == 2 else frozenset()


def absent_cases() -> dict:
    """Schema 2 fields of a receipt that observed no added case."""
    return {field: "absent" if field.endswith(("_stage", "_sha256")) else False
            for field in ADDED_FIELDS}


def added_cases(value: dict) -> list[str]:
    if value["schema_version"] != 2:
        return []
    return [prefix for prefix, point in ADDED_POINTS.items()
            if value[prefix + "interruption_stage"] == point.name]


def case_count(value: dict) -> int:
    """The interruption_case_count a passing receipt must carry."""
    return 1 + len(added_cases(value))


def validate_cases(value: dict) -> None:
    if value["interruption_stage"] not in ("absent", STAGED_RESTORE.name):
        raise ValueError("T22 receipt stage is invalid")
    if value["pass"] and value["interruption_stage"] != STAGED_RESTORE.name:
        raise ValueError("passing T22 receipt lacks authenticated stop stage")
    added = ADDED_POINTS.items() if value["schema_version"] == 2 else ()
    for prefix, point in added:
        stage, flags = value[prefix + "interruption_stage"], [value[prefix + flag] for flag in CASE_FLAGS]
        if (stage not in ("absent", point.name) or any(type(flag) is not bool for flag in flags) or
                hashes.malformed(value, prefix)):
            raise ValueError(f"T22 receipt {prefix}case is malformed")
        if stage == "absent" and (any(flags) or hashes.claimed(value, prefix)):
            raise ValueError(f"T22 receipt {prefix}case claims proof without a stop")
        if value["pass"] and stage != "absent" and not all(flags):
            raise ValueError(f"passing T22 receipt has an unproven {prefix}case")
        if value["pass"] and stage != "absent":
            hashes.validate_passing(value, prefix)
    if value["pass"]:
        logs = [value["stop_fd_log_sha256"]]
        logs += [value[prefix + "stop_fd_log_sha256"] for prefix in added_cases(value)]
        if len(set(logs)) != len(logs):
            raise ValueError("T22 interruption cases share one stop observation")
