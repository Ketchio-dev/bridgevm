"""Pinned PresentMon v2 CPU frame intervals; never a standalone B6 gate."""
import csv
import hashlib
import io
import math
import os
from pathlib import Path
import re
import stat
import statistics

MAX_BYTES = 7_500_000
REQUIRED = {"Application", "ProcessID", "SwapChainAddress", "CPUStartTime",
            "FrameTime", "CPUBusy", "CPUWait"}
NO_CLAIM = {"claim_eligible": False, "criterion_pass": False, "capability_promotion": False}


def number(value, *, positive=False):
    try:
        result = float(value)
    except (ValueError, TypeError) as error:
        raise ValueError("non-numeric frame metric") from error
    if not math.isfinite(result) or result < 0 or (positive and result == 0):
        raise ValueError("frame metric must be finite and in range")
    return result


def authenticated_bytes(path, expected):
    if not re.fullmatch(r"[0-9a-f]{64}", expected):
        raise ValueError("expected CSV SHA-256 is required")
    path = Path(path)
    if path.is_symlink():
        raise ValueError("CSV symlinks are not evidence files")
    flags = os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0) | getattr(os, "O_NONBLOCK", 0)
    with os.fdopen(os.open(path, flags), "rb") as stream:
        info = os.fstat(stream.fileno())
        if not stat.S_ISREG(info.st_mode) or not 0 < info.st_size <= MAX_BYTES:
            raise ValueError("CSV must be a bounded nonempty regular file")
        raw = stream.read(MAX_BYTES + 1)
    if not raw or len(raw) > MAX_BYTES or hashlib.sha256(raw).hexdigest() != expected:
        raise ValueError("CSV size or hash mismatch")
    return raw, (info.st_dev, info.st_ino)


def summarize(path, expected):
    raw, identity = authenticated_bytes(path, expected)
    reader = csv.DictReader(io.StringIO(raw.decode("utf-8-sig")), strict=True)
    header = reader.fieldnames or []
    if len(header) != len(set(header)) or not REQUIRED.issubset(header):
        raise ValueError("missing or duplicate PresentMon v2 columns")
    frames, starts, streams, modes = [], [], set(), set()
    for row in reader:
        if None in row or any(value is None for value in row.values()):
            raise ValueError("CSV row does not match its header")
        if row["Application"].lower() != "dwm.exe":
            raise ValueError("CSV contains a non-DWM process")
        pid, swap = row["ProcessID"], row["SwapChainAddress"]
        if not re.fullmatch(r"[1-9][0-9]*", pid) or not re.fullmatch(r"0[xX][0-9a-fA-F]+", swap) or int(swap, 16) == 0:
            raise ValueError("invalid process or swapchain identity")
        streams.add((int(pid), int(swap, 16)))
        if len(streams) != 1:
            raise ValueError("multiple DWM process/swapchain streams cannot be pooled")
        frame = number(row["FrameTime"], positive=True)
        start = number(row["CPUStartTime"])
        busy, wait = number(row["CPUBusy"]), number(row["CPUWait"])
        # The exporter rounds each field to four decimal places independently.
        if not math.isclose(frame, busy + wait, rel_tol=0, abs_tol=0.0003):
            raise ValueError("FrameTime does not match CPUBusy plus CPUWait")
        if starts and (start <= starts[-1] or
                       not math.isclose(start - starts[-1], frames[-1], rel_tol=0, abs_tol=0.0003)):
            raise ValueError("CPU timeline is unordered or has missing frame rows")
        frames.append(frame)
        starts.append(start)
        if row.get("PresentMode"):
            modes.add(row["PresentMode"])
    if len(frames) < 2:
        raise ValueError("at least two contiguous frame rows are needed even for a diagnostic")
    ordered = sorted(frames)
    mean = statistics.fmean(frames)
    span = starts[-1] - starts[0]
    if not math.isfinite(mean) or not math.isfinite(span):
        raise ValueError("frame statistics overflowed")
    report = {"schema_version": "bridgevm.b6-frame-time-diagnostic.v1", **NO_CLAIM,
              "sha256": expected, "frame_count": len(frames), "metric": "FrameTime",
              "unit": "milliseconds", "mean_ms": mean,
              "p95_nearest_rank_ms": ordered[math.ceil(len(ordered) * 0.95) - 1],
              "min_ms": ordered[0], "max_ms": ordered[-1], "cpu_start_span_ms": span,
              "present_modes": sorted(modes),
              "limitation": "No workload, sample-coverage, glyph-matrix or baseline-provenance proof."}
    return report, identity


def compare(baseline_path, baseline_hash, candidate_path, candidate_hash):
    baseline, first = summarize(baseline_path, baseline_hash)
    candidate, second = summarize(candidate_path, candidate_hash)
    if first == second or baseline_hash == candidate_hash:
        raise ValueError("baseline and candidate must not reuse the same evidence")
    ratios = {key: candidate[key] / baseline[key] for key in ("mean_ms", "p95_nearest_rank_ms")}
    if not all(math.isfinite(value) for value in ratios.values()):
        raise ValueError("comparison ratios overflowed")
    return {"schema_version": "bridgevm.b6-frame-time-comparison.v1", **NO_CLAIM,
            "baseline": baseline, "candidate": candidate, "ratios": ratios,
            "comparison_limit": 1.10,
            "mean_within_ten_percent": candidate["mean_ms"] <= baseline["mean_ms"] * 1.10,
            "p95_within_ten_percent": candidate["p95_nearest_rank_ms"] <= baseline["p95_nearest_rank_ms"] * 1.10,
            "limitation": "Arithmetic comparison only; the fixed complete B6 live gate remains required."}
