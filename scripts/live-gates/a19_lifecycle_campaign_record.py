"""Private per-lane records and lane layout for the A19 T23 lifecycle campaign.

Each lane retains one path-free record under lanes/lane-NN/. The record binds
the job, sealed commit and sealed input hashes, so a lane from another job or
commit cannot be pooled into this campaign.
"""
from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import re

from native_snapshot_restore_public import _reject_constant, _unique_fields
from native_snapshot_restore_seal import read_bounded_regular

LANES = 10
BOOTS = 3
RECORD = "lane-result.json"
RECORD_SCHEMA = "bridgevm.a19-lifecycle-lane.v1"
OWNED = ("prepared-inputs", "live")
IDENTITY = ("job_id", "commit", "input_manifest_sha256", "binary_hash")
PAIR = ("prepared_image_sha256", "prepared_vars_sha256", "exported_disk_sha256",
        "exported_vars_sha256", "final_disk_sha256", "final_vars_sha256")
MARKERS = ("original_marker_sha256", "clobber_marker_sha256", "restored_marker_sha256")
RESULTS = ("snapshot_create_result_sha256", "snapshot_restore_result_sha256",
           "snapshot_export_result_sha256", "snapshot_export_manifest_sha256")
HASHES = (*PAIR, *MARKERS, *RESULTS)
COUNTS = ("boots_attempted", "boots_passed", "natural_shutdown_count")
FIELDS = {"schema", *IDENTITY, "ordinal", "vm_id", "pass", "cleanup_verified", *COUNTS, *HASHES}
SHA256 = re.compile(r"[0-9a-f]{64}\Z")


def lane_name(ordinal: int) -> str:
    return f"lane-{ordinal:02d}"


def vm_id(ordinal: int) -> str:
    return f"a19-campaign-lane-{ordinal:02d}"


def new_record(identity: dict, ordinal: int) -> dict:
    return {"schema": RECORD_SCHEMA, **{field: identity[field] for field in IDENTITY},
            "ordinal": ordinal, "vm_id": vm_id(ordinal), "pass": False, "cleanup_verified": False,
            **dict.fromkeys(COUNTS, 0), **dict.fromkeys(HASHES, "absent")}


def check_lane(lane: dict, hashes: tuple[str, ...]) -> None:
    """One lane's flags, boot accounting and hashes; a pass needs all of them."""
    if any(type(lane[field]) is not bool for field in ("pass", "cleanup_verified")):
        raise ValueError("lane pass and cleanup flags must be boolean")
    counts = tuple(lane[field] for field in COUNTS)
    if any(type(item) is not int for item in counts) or not 0 <= counts[2] <= counts[1] <= counts[0] <= BOOTS:
        raise ValueError("lane boot and natural shutdown accounting is invalid")
    for field in hashes:
        if lane[field] != "absent" and (not isinstance(lane[field], str) or not SHA256.fullmatch(lane[field])):
            raise ValueError(f"lane {field} is not a SHA-256 or absent")
    if lane["pass"] and (
            not lane["cleanup_verified"] or counts != (BOOTS,) * 3
            or any(lane[field] == "absent" for field in hashes)
            or lane["original_marker_sha256"] != lane["restored_marker_sha256"]
            or lane["clobber_marker_sha256"] == lane["original_marker_sha256"]):
        raise ValueError("a passing lane lacks cleanup, three natural shutdowns, a hash or marker restore")


def validate_record(value: object, identity: dict, ordinal: int) -> dict:
    if not isinstance(value, dict) or set(value) != FIELDS or value["schema"] != RECORD_SCHEMA:
        raise ValueError(f"lane {ordinal} record has an unexpected schema or field set")
    for field in IDENTITY:
        if value[field] != identity[field]:
            raise ValueError(f"lane {ordinal} record belongs to a different {field}")
    if type(value["ordinal"]) is not int or value["ordinal"] != ordinal or value["vm_id"] != vm_id(ordinal):
        raise ValueError(f"lane {ordinal} record carries a different lane identity")
    check_lane(value, HASHES)
    return value


def write_record(path: Path, value: dict, identity: dict, ordinal: int) -> None:
    validate_record(value, identity, ordinal)
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    with os.fdopen(descriptor, "w", encoding="utf-8") as output:
        json.dump(value, output, indent=2, sort_keys=True)
        output.write("\n")


def read_records(lanes: Path, identity: dict, count: int) -> list[tuple[dict, str]]:
    """Exactly lanes 1..count, each with a bound record; no extra or missing lane."""
    if type(count) is not int or not 0 <= count <= LANES:
        raise ValueError("campaign lane count is outside the fixed ten lanes")
    if not os.path.lexists(lanes):
        if count:
            raise ValueError("campaign lane records are missing")
        return []
    if lanes.is_symlink() or not lanes.is_dir():
        raise ValueError("campaign lanes directory is unsafe")
    if sorted(entry.name for entry in lanes.iterdir()) != [lane_name(item) for item in range(1, count + 1)]:
        raise ValueError("campaign lane directories differ from the attempted lanes")
    collected = []
    for ordinal in range(1, count + 1):
        directory = lanes / lane_name(ordinal)
        if directory.is_symlink() or not directory.is_dir():
            raise ValueError(f"lane {ordinal} directory is unsafe")
        data = read_bounded_regular(directory / RECORD, 16_384)
        value = json.loads(data.decode("utf-8"), object_pairs_hook=_unique_fields,
                           parse_constant=_reject_constant)
        collected.append((validate_record(value, identity, ordinal), hashlib.sha256(data).hexdigest()))
    return collected


def lanes_clean(output: Path) -> bool:
    """No lane (and no top-level path) still holds a clone, library or export."""
    try:
        if any(os.path.lexists(output / name) for name in OWNED):
            return False
        lanes = output / "lanes"
        if not os.path.lexists(lanes):
            return True
        if lanes.is_symlink() or not lanes.is_dir():
            return False
        for lane in lanes.iterdir():
            if lane.is_symlink() or not lane.is_dir():
                return False
            if any(os.path.lexists(lane / name) for name in OWNED):
                return False
        return True
    except OSError:
        return False
