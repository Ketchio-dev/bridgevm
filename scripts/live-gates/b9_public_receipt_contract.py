"""Strict exported B9 field types and private-backed public verification."""

from __future__ import annotations

import math
from pathlib import Path
import re

INTEGERS = ("pilot_count", "required_workload_count", "frame_count",
            "scanout_sample_count", "distinct_scanout_count", "target_pid")
METRICS = ("cpu_start_span_ms", "p50_nearest_rank_ms",
           "p95_nearest_rank_ms", "p99_nearest_rank_ms")
BOOLEANS = ("cleanup_complete", "source_integrity", "guest_shutdown_observed",
            "pass", "claim_eligible", "criterion_pass", "capability_promotion")
DIRECT = {"bbb_1080p_10s_5MB_av1.webm": "media",
          "PresentMon-2.5.1-x64.exe": "presentmon",
          "bv-b9-vlc-playback.ps1": "guest_script", "bv-b9-control.ps1": "control_script"}
STAGED = set(DIRECT) | {f"b9-vlc-part-{index:02d}.bin" for index in range(10)} | {"b9-vlc-parts.tsv"}


def check_exported_types(value: dict) -> None:
    for name in INTEGERS:
        if name in value and (type(value[name]) is not int or not 0 <= value[name] < 2**63):
            raise ValueError("B9 exported integer differs: " + name)
    for name in METRICS:
        if name in value and (type(value[name]) not in (int, float)
                              or not math.isfinite(value[name]) or value[name] < 0):
            raise ValueError("B9 exported metric differs: " + name)
    for name in BOOLEANS:
        if name in value and type(value[name]) is not bool:
            raise ValueError("B9 exported boolean differs: " + name)


def check_staged_hashes(value: dict) -> None:
    staged = value.get("staged_file_hashes")
    if staged is None and value.get("result_class") != "VLC_PID_PRESENTS_CAPTURED":
        return
    if (not isinstance(staged, dict) or set(staged) != STAGED
            or any(not isinstance(digest, str) or not re.fullmatch(r"[0-9a-f]{64}", digest)
                   for digest in staged.values())
            or any(staged[name] != value["asset_hashes"][key] for name, key in DIRECT.items())):
        raise ValueError("B9 staged share hashes differ from sealed inputs")


def verify_public(value: dict, job: dict, directory: Path, read_json,
                  validate_private, public_view) -> None:
    private = read_json(directory / "receipt.json")
    validate_private(private, job, directory / "diagnostic")
    if value != public_view(private, job):
        raise ValueError("public B9 receipt differs from private-backed view")
