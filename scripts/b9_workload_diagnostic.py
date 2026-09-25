#!/usr/bin/env python3
"""CSV-only B9 candidate diagnostic; never evidence of playback or compatibility."""

from __future__ import annotations

import argparse
import csv
import hashlib
import io
import json
import math
import os
from pathlib import Path
import re
import stat
import sys

ROOT = Path(__file__).resolve().parent.parent
DECLARATION = ROOT / "docs/windows-arm/b9-first-workload-v1.json"
MAX_CSV_BYTES = 7_500_000
MAX_DECLARATION_BYTES = 16_384
REQUIRED = ("Application", "ProcessID", "SwapChainAddress", "CPUStartTime",
            "FrameTime", "CPUBusy", "CPUWait")
NO_CLAIM = {"claim_eligible": False, "criterion_pass": False,
            "capability_promotion": False}
EXPECTED_DECLARATION = {
    "schema_version": "bridgevm.b9-candidate.v1",
    "candidate_only": True,
    "required_workloads": 20,
    "id": "vlc-3.0.23-arm64-kodi-bbb-1080p-av1-10s-v1",
    "application": {
        "exe": "vlc.exe", "asset": "vlc-3.0.23-winarm64.zip",
        "source_url": "https://download.videolan.org/pub/videolan/vlc/3.0.23/winarm64/vlc-3.0.23-winarm64.zip",
        "bytes": 71_151_376,
        "sha256": "9c0917dc521ffc8ce30e70bca7f6c9dc8fec80909d763e75cd976351dee8db0b",
    },
    "media": {
        "asset": "bbb_1080p_10s_5MB_av1.webm",
        "source_url": "https://mirrors.kodi.tv/demo-files/BBB/bbb_1080p_10s_5MB_av1.webm?mirrorlist=",
        "bytes": 5_252_644,
        "sha256": "5e43740e2916afc1b17de09f4948f038b83065a5d42f0fcb16643d7faeb00ad7",
    },
    "collector": {
        "asset": "PresentMon-2.5.1-x64.exe",
        "source_url": "https://github.com/GameTechDev/PresentMon/releases/download/v2.5.1/PresentMon-2.5.1-x64.exe",
        "bytes": 956_768,
        "sha256": "9bec3083069f58f911e6a512f4806db51a27bd096103087bc1d05ef54c80a191",
        "csv_format": "PresentMon v2",
    },
}


class DiagnosticError(ValueError):
    """An input cannot support even a CSV-only diagnostic."""


def bounded_regular_bytes(path: Path, maximum: int) -> bytes:
    """Read one stable regular file without following a symlink."""
    if path.is_symlink():
        raise DiagnosticError("symlink input is refused")
    flags = os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0) | getattr(os, "O_NONBLOCK", 0)
    try:
        with os.fdopen(os.open(path, flags), "rb") as stream:
            before = os.fstat(stream.fileno())
            if not stat.S_ISREG(before.st_mode) or not 0 < before.st_size <= maximum:
                raise DiagnosticError("input must be a bounded nonempty regular file")
            raw = stream.read(maximum + 1)
            after = os.fstat(stream.fileno())
    except OSError as error:
        raise DiagnosticError("input cannot be read as a regular file") from error
    identity = lambda info: (info.st_dev, info.st_ino, info.st_size, info.st_mtime_ns)
    if len(raw) != before.st_size or len(raw) > maximum or identity(before) != identity(after):
        raise DiagnosticError("input changed during its bounded read")
    return raw


def unique_pairs(pairs: list[tuple[str, object]]) -> dict:
    result = {}
    for key, value in pairs:
        if key in result:
            raise DiagnosticError("duplicate candidate declaration key")
        result[key] = value
    return result


def load_declaration(path: Path = DECLARATION) -> tuple[dict, str]:
    raw = bounded_regular_bytes(path, MAX_DECLARATION_BYTES)
    try:
        value = json.loads(raw.decode("utf-8"), object_pairs_hook=unique_pairs)
    except (UnicodeError, json.JSONDecodeError) as error:
        raise DiagnosticError("candidate declaration is malformed") from error
    if value != EXPECTED_DECLARATION:
        raise DiagnosticError("candidate declaration differs from the fixed candidate")
    return value, hashlib.sha256(raw).hexdigest()


def metric(value: str, *, positive: bool = False) -> float:
    if not re.fullmatch(r"[0-9]+(?:\.[0-9]+)?", value):
        raise DiagnosticError("frame metric is not a plain decimal")
    try:
        parsed = float(value)
    except (TypeError, ValueError) as error:
        raise DiagnosticError("frame metric is not numeric") from error
    if not math.isfinite(parsed) or parsed < 0 or (positive and parsed == 0):
        raise DiagnosticError("frame metric is not finite and in range")
    return parsed


def nearest_rank(ordered: list[float], percentile: float) -> float:
    return ordered[math.ceil(len(ordered) * percentile) - 1]


def summarize_csv(path: Path, expected_sha256: str, target_pid: int) -> dict:
    """Check caller-bound bytes, then only self-reported app rows in one stream."""
    if not re.fullmatch(r"[0-9a-f]{64}", expected_sha256):
        raise DiagnosticError("a lowercase CSV SHA-256 is required")
    if isinstance(target_pid, bool) or not isinstance(target_pid, int) or not 0 < target_pid < 2**32:
        raise DiagnosticError("a positive 32-bit target PID is required")
    declaration, declaration_sha256 = load_declaration()
    raw = bounded_regular_bytes(path, MAX_CSV_BYTES)
    if hashlib.sha256(raw).hexdigest() != expected_sha256:
        raise DiagnosticError("CSV bytes do not match the caller-supplied SHA-256")
    try:
        content = raw.decode("utf-8-sig")
    except UnicodeError as error:
        raise DiagnosticError("CSV is not UTF-8") from error
    if "\x00" in content:
        raise DiagnosticError("CSV contains a NUL byte")

    reader = csv.reader(io.StringIO(content, newline=""), strict=True)
    try:
        header = next(reader, None)
        if (not header or len(header) != len(set(header)) or
                not set(REQUIRED).issubset(header) or reader.line_num != 1):
            raise DiagnosticError("CSV has missing, duplicate or multiline columns")
        columns = {name: header.index(name) for name in REQUIRED}
        frames: list[float] = []
        starts: list[float] = []
        swapchain: int | None = None
        previous_line = reader.line_num
        for row in reader:
            if reader.line_num != previous_line + 1 or len(row) != len(header):
                raise DiagnosticError("CSV has a blank, multiline or ragged row")
            previous_line = reader.line_num
            field = lambda name: row[columns[name]]
            if field("Application") != declaration["application"]["exe"]:
                raise DiagnosticError("CSV contains a non-target application row")
            if field("ProcessID") != str(target_pid):
                raise DiagnosticError("CSV contains a non-target PID row")
            address = field("SwapChainAddress")
            if not re.fullmatch(r"0[xX][0-9a-fA-F]{1,16}", address) or int(address, 16) == 0:
                raise DiagnosticError("CSV has an invalid swapchain identity")
            current_swapchain = int(address, 16)
            if swapchain is not None and current_swapchain != swapchain:
                raise DiagnosticError("CSV mixes swapchain streams")
            swapchain = current_swapchain
            frame = metric(field("FrameTime"), positive=True)
            start = metric(field("CPUStartTime"))
            busy = metric(field("CPUBusy"))
            wait = metric(field("CPUWait"))
            # PresentMon v2 exports each millisecond field rounded to 4 places.
            if not math.isclose(frame, busy + wait, rel_tol=0, abs_tol=0.0003):
                raise DiagnosticError("FrameTime disagrees with CPUBusy plus CPUWait")
            if starts and (start <= starts[-1] or not math.isclose(
                    start - starts[-1], frames[-1], rel_tol=0, abs_tol=0.0003)):
                raise DiagnosticError("CPU frame timeline is not ordered and contiguous")
            frames.append(frame)
            starts.append(start)
    except csv.Error as error:
        raise DiagnosticError("CSV syntax is malformed") from error
    if len(frames) < 2:
        raise DiagnosticError("at least two contiguous app rows are needed")
    span = starts[-1] - starts[0]
    if not math.isfinite(span):
        raise DiagnosticError("CPU timeline span is not finite")
    ordered = sorted(frames)
    return {
        "schema_version": "bridgevm.b9-csv-diagnostic.v1",
        "diagnostic_class": "CSV_APP_ROWS_VALID",
        "csv_valid": True,
        "scope": "caller-hash-bound PresentMon CSV rows only",
        "candidate_id": declaration["id"],
        "candidate_declaration_sha256": declaration_sha256,
        "csv_sha256": expected_sha256,
        "target_pid": target_pid,
        "application": declaration["application"]["exe"],
        "swapchain_address": f"0x{swapchain:x}",
        "frame_count": len(frames),
        "metric": "FrameTime",
        "unit": "milliseconds",
        "p50_nearest_rank_ms": nearest_rank(ordered, 0.50),
        "p95_nearest_rank_ms": nearest_rank(ordered, 0.95),
        "p99_nearest_rank_ms": nearest_rank(ordered, 0.99),
        "cpu_start_span_ms": span,
        "source_assets_verified": False,
        "collector_completion_verified": False,
        "playback_verified": False,
        "display_verified": False,
        **NO_CLAIM,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("csv", type=Path)
    parser.add_argument("csv_sha256")
    parser.add_argument("target_pid", type=int)
    args = parser.parse_args()
    try:
        report = summarize_csv(args.csv, args.csv_sha256, args.target_pid)
    except DiagnosticError as error:
        report = {"schema_version": "bridgevm.b9-csv-diagnostic.v1",
                  "diagnostic_class": "INVALID_CSV", "csv_valid": False,
                  "scope": "caller-hash-bound PresentMon CSV rows only",
                  "detail": str(error), **NO_CLAIM}
        print(json.dumps(report, sort_keys=True, allow_nan=False))
        return 1
    print(json.dumps(report, sort_keys=True, allow_nan=False))
    return 0


if __name__ == "__main__":
    sys.exit(main())
