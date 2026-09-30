"""Stop log and content hashes an added T22 case carries, checked like case one.

A case's flags are self-reported. A pass also needs the hashes of what the
kill must not change to agree, the retry's result to be recorded, and the
retry's outcome to differ from the state the kill had to leave, so a helper
that published before its kill could not look unchanged.
"""
from __future__ import annotations

import re

SHA256 = re.compile(r"[0-9a-f]{64}\Z")
CASE_HASHES = {
    "swap_": ("preinterrupt_disk_sha256", "preinterrupt_vars_sha256",
              "postkill_disk_sha256", "postkill_vars_sha256", "retry_result_sha256",
              "postretry_disk_sha256", "postretry_vars_sha256"),
    "create_": ("source_disk_sha256", "source_vars_sha256",
                "preinterrupt_manifest_sha256", "postkill_manifest_sha256", "retry_result_sha256",
                "postretry_manifest_sha256", "postretry_disk_sha256", "postretry_vars_sha256"),
}
# A destination absent before creation must still be absent after the kill.
MAY_BE_ABSENT = frozenset({"create_preinterrupt_manifest_sha256", "create_postkill_manifest_sha256"})
SAME = (("swap_postkill_disk_sha256", "swap_preinterrupt_disk_sha256"),
        ("swap_postkill_vars_sha256", "swap_preinterrupt_vars_sha256"),
        ("swap_postretry_disk_sha256", "snapshot_disk_sha256"),
        ("swap_postretry_vars_sha256", "snapshot_vars_sha256"),
        ("create_postkill_manifest_sha256", "create_preinterrupt_manifest_sha256"),
        ("create_postretry_disk_sha256", "create_source_disk_sha256"),
        ("create_postretry_vars_sha256", "create_source_vars_sha256"))
DISTINCT = (("swap_preinterrupt_disk_sha256", "snapshot_disk_sha256"),
            ("create_postretry_manifest_sha256", "create_preinterrupt_manifest_sha256"))


def proof_fields(prefix: str) -> tuple[str, ...]:
    return tuple(prefix + field for field in ("stop_fd_log_sha256", *CASE_HASHES[prefix]))


PROOF_FIELDS = frozenset(field for prefix in CASE_HASHES for field in proof_fields(prefix))


def malformed(value: dict, prefix: str) -> bool:
    return any(not isinstance(value[field], str) or
               (value[field] != "absent" and not SHA256.fullmatch(value[field]))
               for field in proof_fields(prefix))


def claimed(value: dict, prefix: str) -> bool:
    return any(value[field] != "absent" for field in proof_fields(prefix))


def validate_passing(value: dict, prefix: str) -> None:
    if any(value[field] == "absent" for field in proof_fields(prefix) if field not in MAY_BE_ABSENT):
        raise ValueError(f"passing T22 receipt lacks {prefix}case hashes")
    for field, source in SAME:
        if field.startswith(prefix) and value[field] != value[source]:
            raise ValueError(f"passing T22 receipt changed {field}")
    for field, other in DISTINCT:
        if field.startswith(prefix) and value[field] == value[other]:
            raise ValueError(f"passing T22 receipt cannot tell {field} from {other}")
