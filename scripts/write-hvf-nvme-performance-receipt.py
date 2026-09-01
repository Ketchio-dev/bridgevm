#!/usr/bin/env python3
"""Validate private T16 NVMe artifacts and emit an aggregate-only receipt."""

from __future__ import annotations

import argparse
import datetime
import hashlib
import json
import math
import os
import re
import sys
import tempfile
from pathlib import Path
from typing import Any

SCHEMA = "bridgevm.windows-nvme-warm-seq.v1"
WORKLOAD = "windows-nvme-warm-seq-v1"
TIER = "t16-hvf-nvme-performance"
GATE_ID = "hvf-nvme-performance-diagnostic"
FILE_BYTES, TRANSFER_BYTES, BLOCKS_PER_PASS = 536_870_912, 131_072, 4_096
READ_RESULTS, WRITE_RESULTS, READ_OPS, WRITE_OPS = 5, 2, 20_480, 8_192
KNOWN_CONFOUNDERS = [
    "guest-unbuffered sequential I/O uses a host-warm backing image",
    "storage and desktop timing share one end-to-end live attempt",
    "a host power-source event monitor runs concurrently",
]
CONFIG = {
    "schema": SCHEMA, "workload_profile": WORKLOAD,
    "pattern_id": "offset-xorshift64-v1", "file_mib": 512,
    "transfer_kib": 128, "read_passes": 5, "write_passes": 2,
    "queue_depth": 1, "read_flags": ["FILE_FLAG_NO_BUFFERING"],
    "write_flags": ["FILE_FLAG_NO_BUFFERING", "FILE_FLAG_WRITE_THROUGH"],
    "flush_semantics": "FlushFileBuffers after precondition and each measured write pass",
    "verification_semantics": "full readback after every measured write pass",
    "cache_profile": "guest-unbuffered-host-warm",
}
CONFIG_SHA256 = hashlib.sha256(json.dumps(CONFIG, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
RAW_KEYS = {"schema", "workload_profile", "nonce", "config_sha256", "read_latency_ns", "write_latency_ns", "flush_latency_ns"}
DONE_KEYS = {"schema", "workload_profile", "nonce", "config_sha256", "status", "raw_sha256", "result_sha256"}
LATENCY_KEYS = {"count", "p50", "p95", "p99", "max"}
RESULT_KEYS = {
    "schema", "workload_profile", "nonce", "config_sha256", "config", "status",
    "raw_sha256", "file_bytes", "transfer_bytes", "blocks_per_pass",
    "precondition_write_ops", "precondition_write_bytes", "warmup_read_ops",
    "warmup_read_bytes", "measured_read_ops", "measured_read_bytes",
    "measured_write_ops", "measured_write_bytes", "write_verify_read_ops", "read_result_count",
    "write_result_count", "final_verify_read_ops", "final_verify_read_bytes",
    "verified_read_ops", "flush_calls", "bytes_per_sector",
    "file_alignment_bytes", "required_alignment_bytes", "warmup_sha256",
    "final_sha256", "read_phase_elapsed_ns", "read_service_elapsed_ns",
    "read_phase_mib_per_sec", "read_service_mib_per_sec", "read_throughput_mib_s",
    "read_p50_ms", "read_p95_ms", "read_p99_ms", "read_max_ms",
    "write_phase_elapsed_ns", "write_and_flush_service_elapsed_ns",
    "write_durable_mib_per_sec", "write_and_flush_service_mib_per_sec",
    "write_durable_throughput_mib_s", "write_p50_ms", "write_p95_ms",
    "write_p99_ms", "write_max_ms", "flush_p50_ms", "flush_p95_ms",
    "flush_p99_ms", "flush_max_ms", "read_latency_ns", "write_latency_ns",
    "flush_latency_ns",
}
PERFORMANCE_FIELDS = (
    "read_phase_mib_per_sec", "read_service_mib_per_sec", "read_throughput_mib_s",
    "read_p50_ms", "read_p95_ms", "read_p99_ms", "read_max_ms",
    "write_durable_mib_per_sec", "write_and_flush_service_mib_per_sec",
    "write_durable_throughput_mib_s", "write_p50_ms", "write_p95_ms",
    "write_p99_ms", "write_max_ms", "flush_p50_ms", "flush_p95_ms",
    "flush_p99_ms", "flush_max_ms",
)
COUNT_FIELDS = (
    "file_bytes", "transfer_bytes", "blocks_per_pass", "precondition_write_ops",
    "precondition_write_bytes", "warmup_read_ops", "warmup_read_bytes",
    "measured_read_ops", "measured_read_bytes", "measured_write_ops",
    "measured_write_bytes", "write_verify_read_ops", "read_result_count", "write_result_count",
    "final_verify_read_ops", "final_verify_read_bytes", "verified_read_ops",
    "flush_calls", "bytes_per_sector", "file_alignment_bytes",
    "required_alignment_bytes", "read_phase_elapsed_ns", "read_service_elapsed_ns",
    "write_phase_elapsed_ns", "write_and_flush_service_elapsed_ns",
)
HASH_FIELDS = (
    "binary_hash", "input_manifest_sha256", "image_sha256", "vars_sha256",
    "firmware_sha256", "renderer_sha256", "config_sha256", "power_log_sha256",
    "campaign_registry_sha256",
    "workload_script_sha256", "raw_sha256", "result_sha256", "done_sha256",
    "warmup_sha256", "final_sha256",
)
RECEIPT_KEYS = {
    "schema_version", "tier", "gate_id", "job_id", "commit", "harness_commit",
    *HASH_FIELDS, "workload_profile", "nonce", "campaign_id",
    "campaign_mode", "campaign_role", "campaign_ordinal", "campaign_expected_runs",
    "host_model", "macos_version", "power_source", "power_source_start",
    "power_source_end", "file_mib", "transfer_kib", "read_passes", "write_passes",
    "smp_cpus", "ram_mib", "known_confounders", *COUNT_FIELDS,
    *PERFORMANCE_FIELDS, "desktop_elapsed_ms", "sample_count",
    "run_count", "required_run_count", "passes", "failures", "started_at",
    "finished_at", "outcome", "pass", "valid", "invalid_reason", "evidence_paths",
}
FAILURE_REASONS = {
    "artifact-invalid", "artifact-missing", "guest-unreachable", "power-source-invalid",
    "workload-failed", "workload-timeout", "worker-interrupted",
}


def _required(name: str) -> str:
    value = os.environ.get(name, "")
    if not value:
        raise ValueError(f"missing receipt field {name}")
    return value


def _hex(value: Any, width: int) -> bool:
    return isinstance(value, str) and bool(re.fullmatch(f"[0-9a-f]{{{width}}}", value))


def _integer(value: Any, name: str) -> int:
    if type(value) is not int or value <= 0:
        raise ValueError(f"{name} must be a positive integer")
    return value


def _number(value: Any, name: str) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise ValueError(f"{name} must be numeric")
    converted = float(value)
    if not math.isfinite(converted) or converted <= 0:
        raise ValueError(f"{name} must be finite and positive")
    return converted


def _timestamp(value: Any, name: str) -> datetime.datetime:
    if not isinstance(value, str):
        raise ValueError(f"{name} must be an ISO-8601 timestamp")
    try:
        parsed = datetime.datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError as exc:
        raise ValueError(f"{name} must be an ISO-8601 timestamp") from exc
    if parsed.tzinfo is None:
        raise ValueError(f"{name} must include a timezone")
    return parsed


def _read_artifact(path: Path, label: str) -> tuple[dict[str, Any], str]:
    if not path.is_file() or path.is_symlink():
        raise ValueError(f"{label} must be a regular, non-symlink file")
    payload = path.read_bytes()
    if len(payload) >= 8 * 1024 * 1024 or payload.startswith(b"\xef\xbb\xbf"):
        raise ValueError(f"{label} violates the private artifact boundary")
    try:
        text = payload.decode("utf-8")
        document = json.loads(text)
    except (UnicodeError, json.JSONDecodeError) as exc:
        raise ValueError(f"{label} is not valid UTF-8 JSON") from exc
    if not isinstance(document, dict) or text != text.strip() or "\n" in text or "\r" in text:
        raise ValueError(f"{label} must contain one compact JSON object")
    return document, hashlib.sha256(payload).hexdigest()


def _read_receipt(path: Path) -> dict[str, Any]:
    if not path.is_file() or path.is_symlink():
        raise ValueError("receipt must be a regular, non-symlink file")
    try:
        document = json.loads(path.read_text(encoding="utf-8"))
    except (UnicodeError, json.JSONDecodeError) as exc:
        raise ValueError("receipt is not valid UTF-8 JSON") from exc
    if not isinstance(document, dict):
        raise ValueError("receipt must contain one JSON object")
    return document


def _fixed_identity(document: dict[str, Any], keys: set[str], label: str) -> None:
    if set(document) != keys:
        raise ValueError(f"{label} has missing or unknown fields")
    if document.get("schema") != SCHEMA or document.get("workload_profile") != WORKLOAD:
        raise ValueError(f"{label} has an invalid schema or workload profile")
    if not isinstance(document.get("nonce"), str) or not re.fullmatch(r"[0-9a-f]{16,64}", document["nonce"]):
        raise ValueError(f"{label} nonce is not canonical")
    if document.get("config_sha256") != CONFIG_SHA256:
        raise ValueError(f"{label} config_sha256 does not match the fixed config")


def _summary(values: list[int]) -> dict[str, int]:
    ordered = sorted(values)
    rank = lambda q: ordered[max(0, math.ceil(q * len(ordered)) - 1)]
    return {"count": len(values), "p50": rank(.50), "p95": rank(.95), "p99": rank(.99), "max": ordered[-1]}


def validate_artifacts(result: dict[str, Any], result_hash: str, raw: dict[str, Any], raw_hash: str, done: dict[str, Any], done_hash: str) -> dict[str, str]:
    _fixed_identity(raw, RAW_KEYS, "raw JSON")
    _fixed_identity(result, RESULT_KEYS, "result JSON")
    _fixed_identity(done, DONE_KEYS, "done JSON")
    identity = (result["nonce"], result["config_sha256"], result["workload_profile"])
    if identity != (raw["nonce"], raw["config_sha256"], raw["workload_profile"]) or identity != (done["nonce"], done["config_sha256"], done["workload_profile"]):
        raise ValueError("raw/result/done workload identities disagree")
    if result.get("status") != "passed" or done.get("status") != "passed":
        raise ValueError("result and done status must both be passed")
    if result.get("raw_sha256") != raw_hash or done.get("raw_sha256") != raw_hash:
        raise ValueError("result and done do not seal the exact raw JSON bytes")
    if done.get("result_sha256") != result_hash:
        raise ValueError("done does not seal the exact result JSON bytes")
    if result.get("config") != CONFIG:
        raise ValueError("result config is not the exact fixed workload config")
    vectors: dict[str, list[int]] = {}
    for field, count in (("read_latency_ns", READ_OPS), ("write_latency_ns", WRITE_OPS), ("flush_latency_ns", 2)):
        values = raw.get(field)
        if not isinstance(values, list) or len(values) != count:
            raise ValueError(f"raw {field} must contain exactly {count} samples")
        if any(type(value) is not int or value <= 0 for value in values):
            raise ValueError(f"raw {field} samples must be positive integer nanoseconds")
        vectors[field] = values
        nested = result.get(field)
        if not isinstance(nested, dict) or set(nested) != LATENCY_KEYS or nested != _summary(values):
            raise ValueError(f"result {field} does not summarize the sealed raw vector")
    fixed = {
        "file_bytes": FILE_BYTES, "transfer_bytes": TRANSFER_BYTES,
        "blocks_per_pass": BLOCKS_PER_PASS, "precondition_write_ops": BLOCKS_PER_PASS,
        "precondition_write_bytes": FILE_BYTES, "warmup_read_ops": BLOCKS_PER_PASS,
        "warmup_read_bytes": FILE_BYTES, "measured_read_ops": READ_OPS,
        "measured_read_bytes": FILE_BYTES * READ_RESULTS,
        "measured_write_ops": WRITE_OPS, "measured_write_bytes": FILE_BYTES * WRITE_RESULTS,
        "write_verify_read_ops": WRITE_OPS,
        "read_result_count": READ_RESULTS, "write_result_count": WRITE_RESULTS,
        "final_verify_read_ops": BLOCKS_PER_PASS, "final_verify_read_bytes": FILE_BYTES,
        "verified_read_ops": READ_OPS + WRITE_OPS + BLOCKS_PER_PASS,
        "flush_calls": WRITE_RESULTS + 1,
    }
    for field, expected in fixed.items():
        if result.get(field) != expected or type(result.get(field)) is not int:
            raise ValueError(f"result has invalid fixed count {field}")
    for field in ("bytes_per_sector", "file_alignment_bytes", "required_alignment_bytes"):
        value = _integer(result.get(field), field)
        if value & (value - 1):
            raise ValueError(f"result {field} must be a power of two")
    if result["required_alignment_bytes"] != max(result["bytes_per_sector"], result["file_alignment_bytes"]) or TRANSFER_BYTES % result["required_alignment_bytes"]:
        raise ValueError("result storage alignment is inconsistent")
    for field in ("warmup_sha256", "final_sha256"):
        if not _hex(result.get(field), 64):
            raise ValueError(f"result {field} is not canonical")
    for field in ("read_phase_elapsed_ns", "read_service_elapsed_ns", "write_phase_elapsed_ns", "write_and_flush_service_elapsed_ns"):
        _integer(result.get(field), field)
    if result["read_service_elapsed_ns"] != sum(vectors["read_latency_ns"]):
        raise ValueError("read_service_elapsed_ns does not match raw samples")
    if result["write_and_flush_service_elapsed_ns"] != sum(vectors["write_latency_ns"]) + sum(vectors["flush_latency_ns"]):
        raise ValueError("write_and_flush_service_elapsed_ns does not match raw samples")
    if result["read_phase_elapsed_ns"] < result["read_service_elapsed_ns"] or result["write_phase_elapsed_ns"] < result["write_and_flush_service_elapsed_ns"]:
        raise ValueError("phase elapsed time is shorter than measured service time")
    expected_rates = {
        "read_phase_mib_per_sec": 2560.0 / (result["read_phase_elapsed_ns"] / 1e9),
        "read_service_mib_per_sec": 2560.0 / (result["read_service_elapsed_ns"] / 1e9),
        "write_durable_mib_per_sec": 1024.0 / (result["write_phase_elapsed_ns"] / 1e9),
        "write_and_flush_service_mib_per_sec": 1024.0 / (result["write_and_flush_service_elapsed_ns"] / 1e9),
    }
    for field, expected in expected_rates.items():
        if not math.isclose(_number(result.get(field), field), expected, rel_tol=1e-12, abs_tol=1e-12):
            raise ValueError(f"result {field} is inconsistent with elapsed time and bytes")
    for field, source in (("read_throughput_mib_s", "read_phase_mib_per_sec"), ("write_durable_throughput_mib_s", "write_durable_mib_per_sec")):
        if _number(result.get(field), field) != result[source]:
            raise ValueError(f"result {field} disagrees with its canonical throughput")
    for prefix in ("read", "write", "flush"):
        for percentile in ("p50", "p95", "p99", "max"):
            field = f"{prefix}_{percentile}_ms"
            if _number(result.get(field), field) != result[f"{prefix}_latency_ns"][percentile] / 1e6:
                raise ValueError(f"result {field} disagrees with the raw latency summary")
    return {"raw_sha256": raw_hash, "result_sha256": result_hash, "done_sha256": done_hash}


def _load_artifacts(result_path: Path, raw_path: Path, done_path: Path, config_path: Path) -> tuple[dict[str, Any], dict[str, str]]:
    if not config_path.is_file() or config_path.is_symlink():
        raise ValueError("config JSON must be a regular, non-symlink file")
    config_bytes = config_path.read_bytes()
    expected_config = json.dumps(CONFIG, sort_keys=True, separators=(",", ":")).encode()
    if config_bytes != expected_config or hashlib.sha256(config_bytes).hexdigest() != CONFIG_SHA256:
        raise ValueError("config JSON is not the exact canonical fixed config")
    result, result_hash = _read_artifact(result_path, "result JSON")
    raw, raw_hash = _read_artifact(raw_path, "raw JSON")
    done, done_hash = _read_artifact(done_path, "done JSON")
    return result, validate_artifacts(result, result_hash, raw, raw_hash, done, done_hash)


def _campaign_shape(ordinal: int, expected: int, mode: str, role: str) -> None:
    if expected < 20 or expected > 200 or expected % 4 or ordinal not in range(1, expected + 1):
        raise ValueError("campaign must have a multiple-of-four run count in 20..200")
    expected_role = "baseline" if (((ordinal - 1) // 2 + ordinal) % 2 == 1) else "candidate"
    if mode not in {"AA", "AB"} or role != expected_role:
        raise ValueError("campaign mode or counterbalanced role is invalid")


def _identity() -> dict[str, Any]:
    job_id = _required("NVME_PERF_JOB_ID")
    if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]{0,127}", job_id):
        raise ValueError("NVME_PERF_JOB_ID is not canonical")
    try:
        ordinal_text = _required("NVME_PERF_CAMPAIGN_ORDINAL")
        expected_text = _required("NVME_PERF_CAMPAIGN_EXPECTED_RUNS")
        ordinal, expected = int(ordinal_text), int(expected_text)
        if str(ordinal) != ordinal_text or str(expected) != expected_text:
            raise ValueError("campaign ordinal and expected runs must be canonical integers")
    except ValueError as exc:
        raise ValueError("campaign ordinal and expected runs must be integers") from exc
    mode, role = _required("NVME_PERF_CAMPAIGN_MODE"), _required("NVME_PERF_CAMPAIGN_ROLE")
    _campaign_shape(ordinal, expected, mode, role)
    harness = _required("NVME_PERF_HARNESS_COMMIT")
    if not _hex(harness, 40):
        raise ValueError("harness commit must be 40 lowercase hex characters")
    hashes = {
        "binary_hash": _required("NVME_PERF_BINARY_HASH"),
        "input_manifest_sha256": _required("NVME_PERF_MANIFEST_HASH"),
        "image_sha256": _required("NVME_PERF_IMAGE_HASH"),
        "vars_sha256": _required("NVME_PERF_VARS_HASH"),
        "firmware_sha256": _required("NVME_PERF_FIRMWARE_HASH"),
        "renderer_sha256": _required("NVME_PERF_RENDERER_HASH"),
        "config_sha256": _required("NVME_PERF_CONFIG_HASH"),
        "workload_script_sha256": _required("NVME_PERF_WORKLOAD_SCRIPT_HASH"),
        "power_log_sha256": _required("NVME_PERF_POWER_LOG_HASH"),
        "campaign_registry_sha256": _required("NVME_PERF_CAMPAIGN_REGISTRY_HASH"),
    }
    if hashes["config_sha256"] != CONFIG_SHA256 or any(not _hex(value, 64) for value in hashes.values()):
        raise ValueError("sealed hashes are non-canonical or do not match the fixed config")
    campaign_id = _required("NVME_PERF_CAMPAIGN_ID")
    if not _hex(campaign_id, 32):
        raise ValueError("campaign id must be 32 lowercase hex characters")
    started_at = _required("NVME_PERF_STARTED_AT")
    started = _timestamp(started_at, "NVME_PERF_STARTED_AT")
    finished = datetime.datetime.now(datetime.timezone.utc)
    if finished <= started:
        raise ValueError("NVME_PERF_STARTED_AT must precede receipt creation")
    return {
        "job_id": job_id, "commit": harness, "harness_commit": harness, **hashes,
        "campaign_id": campaign_id, "campaign_mode": mode, "campaign_role": role,
        "campaign_ordinal": ordinal, "campaign_expected_runs": expected,
        "host_model": _required("NVME_PERF_HOST_MODEL"),
        "macos_version": _required("NVME_PERF_MACOS_VERSION"),
        "power_source": _required("NVME_PERF_POWER_SOURCE_START"),
        "power_source_start": _required("NVME_PERF_POWER_SOURCE_START"),
        "power_source_end": _required("NVME_PERF_POWER_SOURCE_END"),
        "started_at": started_at, "finished_at": finished.isoformat().replace("+00:00", "Z"),
    }


def _base(identity: dict[str, Any]) -> dict[str, Any]:
    return {"schema_version": 1, "tier": TIER, "gate_id": GATE_ID, **identity,
            "workload_profile": WORKLOAD, "file_mib": 512, "transfer_kib": 128,
            "read_passes": 5, "write_passes": 2, "smp_cpus": 4,
            "ram_mib": 6144, "known_confounders": KNOWN_CONFOUNDERS}


def _build_receipt(result: dict[str, Any], artifact_hashes: dict[str, str]) -> dict[str, Any]:
    identity = _identity()
    expected_nonce = hashlib.sha256(f"{identity['campaign_id']}:{identity['campaign_ordinal']}".encode()).hexdigest()[:32]
    if result.get("nonce") != expected_nonce:
        raise ValueError("workload nonce does not match the sealed campaign lane")
    if any(identity[field] != "AC Power" for field in ("power_source", "power_source_start", "power_source_end")):
        raise ValueError("T16 measurements require AC Power throughout")
    copied = {field: result[field] for field in COUNT_FIELDS + PERFORMANCE_FIELDS}
    receipt = {
        **_base(identity), **artifact_hashes, "warmup_sha256": result["warmup_sha256"],
        "final_sha256": result["final_sha256"], "nonce": result["nonce"], **copied,
        "desktop_elapsed_ms": _number(float(_required("NVME_PERF_DESKTOP_ELAPSED_MS")), "desktop_elapsed_ms"),
        "sample_count": 1, "run_count": 1, "required_run_count": 1,
        "passes": 1, "failures": 0, "outcome": "completed", "pass": True,
        "valid": True, "invalid_reason": "", "evidence_paths": ["share/nvme-result.json", "power-source.log"],
    }
    validate_receipt(receipt, result, artifact_hashes)
    return receipt


def _build_failure(reason: str) -> dict[str, Any]:
    if reason not in FAILURE_REASONS:
        raise ValueError("failure reason is not an allowed canonical token")
    identity = _identity()
    empty = {field: None for field in PERFORMANCE_FIELDS}
    counts = {field: 0 for field in COUNT_FIELDS}
    return {
        **_base(identity), "raw_sha256": None, "result_sha256": None,
        "done_sha256": None, "warmup_sha256": None, "final_sha256": None,
        "nonce": None, **counts, **empty, "desktop_elapsed_ms": None,
        "sample_count": 0, "run_count": 0, "required_run_count": 1,
        "passes": 0, "failures": 1, "outcome": "failed", "pass": False,
        "valid": False, "invalid_reason": reason, "evidence_paths": [],
    }


def validate_receipt(receipt: dict[str, Any], result: dict[str, Any] | None = None, artifact_hashes: dict[str, str] | None = None) -> None:
    if set(receipt) != RECEIPT_KEYS:
        raise ValueError("receipt has missing or unknown fields")
    fixed = {"schema_version": 1, "tier": TIER, "gate_id": GATE_ID, "workload_profile": WORKLOAD,
             "file_mib": 512, "transfer_kib": 128, "read_passes": 5, "write_passes": 2,
             "required_run_count": 1, "smp_cpus": 4, "ram_mib": 6144,
             "known_confounders": KNOWN_CONFOUNDERS}
    for field, expected in fixed.items():
        if receipt.get(field) != expected or (type(expected) is int and type(receipt.get(field)) is not int):
            raise ValueError(f"receipt has invalid {field}")
    for field in ("binary_hash", "input_manifest_sha256", "image_sha256", "vars_sha256", "firmware_sha256", "renderer_sha256", "config_sha256", "workload_script_sha256", "power_log_sha256", "campaign_registry_sha256"):
        if not _hex(receipt.get(field), 64):
            raise ValueError(f"receipt {field} is not canonical")
    if receipt["config_sha256"] != CONFIG_SHA256:
        raise ValueError("receipt config_sha256 is not the fixed workload config")
    for field in ("commit", "harness_commit"):
        if not _hex(receipt.get(field), 40):
            raise ValueError(f"receipt {field} is not canonical")
    if receipt["commit"] != receipt["harness_commit"]:
        raise ValueError("receipt harness commit aliases disagree")
    if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]{0,127}", receipt.get("job_id", "")) or not _hex(receipt.get("campaign_id"), 32):
        raise ValueError("receipt job or campaign id is not canonical")
    _campaign_shape(_integer(receipt.get("campaign_ordinal"), "campaign_ordinal"), _integer(receipt.get("campaign_expected_runs"), "campaign_expected_runs"), receipt.get("campaign_mode"), receipt.get("campaign_role"))
    for field in ("host_model", "macos_version"):
        if not isinstance(receipt.get(field), str) or receipt[field].strip().lower() in {"", "unknown"}:
            raise ValueError(f"receipt has missing or unknown {field}")
    for field in ("power_source_start", "power_source_end"):
        if not isinstance(receipt.get(field), str) or not receipt[field].strip():
            raise ValueError(f"receipt has missing {field}")
    if receipt.get("power_source") != receipt.get("power_source_start"):
        raise ValueError("receipt power_source alias disagrees")
    if _timestamp(receipt.get("finished_at"), "finished_at") <= _timestamp(receipt.get("started_at"), "started_at"):
        raise ValueError("receipt finished_at must be after started_at")
    if receipt.get("pass") is False:
        if receipt.get("valid") is not False or receipt.get("outcome") != "failed" or receipt.get("invalid_reason") not in FAILURE_REASONS:
            raise ValueError("failed receipt outcome is not canonical")
        if any(receipt.get(field) is not None for field in PERFORMANCE_FIELDS + ("desktop_elapsed_ms", "raw_sha256", "result_sha256", "done_sha256", "warmup_sha256", "final_sha256", "nonce")):
            raise ValueError("failed receipt must not publish incomplete metrics or result hashes")
        if any(receipt.get(field) != 0 for field in COUNT_FIELDS + ("sample_count", "run_count", "passes")) or receipt.get("failures") != 1 or receipt.get("evidence_paths") != []:
            raise ValueError("failed receipt counts are not canonical")
        power_unknown = receipt["power_source_start"].lower() == "unknown" or receipt["power_source_end"].lower() == "unknown"
        if power_unknown and receipt["invalid_reason"] not in {"power-source-invalid", "artifact-invalid"}:
            raise ValueError("unknown failure power source is not allowed for this reason")
        return
    if receipt.get("pass") is not True or receipt.get("valid") is not True or result is None or artifact_hashes is None:
        raise ValueError("passing receipt requires its sealed private artifacts")
    passing = {"sample_count": 1, "run_count": 1, "passes": 1, "failures": 0,
               "outcome": "completed", "invalid_reason": "", "evidence_paths": ["share/nvme-result.json", "power-source.log"]}
    for field, expected in passing.items():
        if receipt.get(field) != expected:
            raise ValueError(f"passing receipt has invalid {field}")
    if any(receipt.get(field) != "AC Power" for field in ("power_source", "power_source_start", "power_source_end")):
        raise ValueError("passing receipt was not measured entirely on AC Power")
    for field, expected in artifact_hashes.items():
        if receipt.get(field) != expected:
            raise ValueError(f"receipt does not seal the exact {field.removesuffix('_sha256')} bytes")
    for field in COUNT_FIELDS + PERFORMANCE_FIELDS + ("warmup_sha256", "final_sha256", "nonce"):
        if receipt.get(field) != result.get(field):
            raise ValueError(f"receipt {field} does not match the result JSON")
    for field in ("raw_sha256", "result_sha256", "done_sha256", "warmup_sha256", "final_sha256"):
        if not _hex(receipt.get(field), 64):
            raise ValueError(f"receipt {field} is not canonical")
    _number(receipt.get("desktop_elapsed_ms"), "desktop_elapsed_ms")


def _fixture(root: Path) -> tuple[Path, Path, Path, Path]:
    nonce = hashlib.sha256(f"{'0' * 32}:1".encode()).hexdigest()[:32]
    reads, writes, flushes = list(range(1_000, 1_000 + READ_OPS)), list(range(2_000, 2_000 + WRITE_OPS)), [3_000, 4_000]
    raw = {"schema": SCHEMA, "workload_profile": WORKLOAD, "nonce": nonce, "config_sha256": CONFIG_SHA256,
           "read_latency_ns": reads, "write_latency_ns": writes, "flush_latency_ns": flushes}
    raw_path = root / "raw.json"; raw_path.write_text(json.dumps(raw, separators=(",", ":")))
    raw_hash = hashlib.sha256(raw_path.read_bytes()).hexdigest()
    read_phase, write_service = sum(reads) + 1_000_000, sum(writes) + sum(flushes)
    write_phase = write_service + 1_000_000
    result: dict[str, Any] = {
        "schema": SCHEMA, "workload_profile": WORKLOAD, "nonce": nonce,
        "config_sha256": CONFIG_SHA256, "config": CONFIG, "status": "passed", "raw_sha256": raw_hash,
        "file_bytes": FILE_BYTES, "transfer_bytes": TRANSFER_BYTES, "blocks_per_pass": BLOCKS_PER_PASS,
        "precondition_write_ops": BLOCKS_PER_PASS, "precondition_write_bytes": FILE_BYTES,
        "warmup_read_ops": BLOCKS_PER_PASS, "warmup_read_bytes": FILE_BYTES,
        "measured_read_ops": READ_OPS, "measured_read_bytes": FILE_BYTES * 5,
        "measured_write_ops": WRITE_OPS, "measured_write_bytes": FILE_BYTES * 2,
        "write_verify_read_ops": WRITE_OPS, "read_result_count": 5, "write_result_count": 2,
        "final_verify_read_ops": BLOCKS_PER_PASS, "final_verify_read_bytes": FILE_BYTES,
        "verified_read_ops": 32_768, "flush_calls": 3,
        "bytes_per_sector": 512, "file_alignment_bytes": 4096, "required_alignment_bytes": 4096,
        "warmup_sha256": "a" * 64, "final_sha256": "b" * 64,
        "read_phase_elapsed_ns": read_phase, "read_service_elapsed_ns": sum(reads),
        "write_phase_elapsed_ns": write_phase, "write_and_flush_service_elapsed_ns": write_service,
        "read_latency_ns": _summary(reads), "write_latency_ns": _summary(writes), "flush_latency_ns": _summary(flushes),
    }
    result.update({"read_phase_mib_per_sec": 2560.0 / (read_phase / 1e9),
                   "read_service_mib_per_sec": 2560.0 / (sum(reads) / 1e9),
                   "write_durable_mib_per_sec": 1024.0 / (write_phase / 1e9),
                   "write_and_flush_service_mib_per_sec": 1024.0 / (write_service / 1e9)})
    result["read_throughput_mib_s"] = result["read_phase_mib_per_sec"]
    result["write_durable_throughput_mib_s"] = result["write_durable_mib_per_sec"]
    for prefix in ("read", "write", "flush"):
        for percentile in ("p50", "p95", "p99", "max"):
            result[f"{prefix}_{percentile}_ms"] = result[f"{prefix}_latency_ns"][percentile] / 1e6
    result_path = root / "result.json"; result_path.write_text(json.dumps(result, separators=(",", ":")))
    result_hash = hashlib.sha256(result_path.read_bytes()).hexdigest()
    done = {"schema": SCHEMA, "workload_profile": WORKLOAD, "nonce": nonce, "config_sha256": CONFIG_SHA256,
            "status": "passed", "raw_sha256": raw_hash, "result_sha256": result_hash}
    done_path = root / "done.json"; done_path.write_text(json.dumps(done, separators=(",", ":")))
    config_path = root / "config.json"
    config_path.write_bytes(json.dumps(CONFIG, sort_keys=True, separators=(",", ":")).encode())
    return result_path, raw_path, done_path, config_path


def self_test() -> None:
    assert CONFIG_SHA256 == "70da5472a5a7e89830ff240cfcb55dd3fff2b84e311016ac096d958879ec4c79"
    with tempfile.TemporaryDirectory() as temporary:
        result_path, raw_path, done_path, config_path = _fixture(Path(temporary))
        result, hashes = _load_artifacts(result_path, raw_path, done_path, config_path)
        assert hashes["result_sha256"] == hashlib.sha256(result_path.read_bytes()).hexdigest()
        previous = os.environ.copy()
        started = (datetime.datetime.now(datetime.timezone.utc) - datetime.timedelta(minutes=1)).isoformat().replace("+00:00", "Z")
        try:
            os.environ.update({
                "NVME_PERF_JOB_ID": "fixture-1", "NVME_PERF_CAMPAIGN_ORDINAL": "1",
                "NVME_PERF_CAMPAIGN_EXPECTED_RUNS": "20", "NVME_PERF_CAMPAIGN_MODE": "AA",
                "NVME_PERF_CAMPAIGN_ROLE": "baseline", "NVME_PERF_HARNESS_COMMIT": "1" * 40,
                "NVME_PERF_BINARY_HASH": "3" * 64,
                "NVME_PERF_MANIFEST_HASH": "4" * 64, "NVME_PERF_IMAGE_HASH": "5" * 64,
                "NVME_PERF_VARS_HASH": "6" * 64, "NVME_PERF_FIRMWARE_HASH": "7" * 64,
                "NVME_PERF_RENDERER_HASH": "8" * 64, "NVME_PERF_CONFIG_HASH": CONFIG_SHA256,
                "NVME_PERF_WORKLOAD_SCRIPT_HASH": "9" * 64, "NVME_PERF_CAMPAIGN_ID": "0" * 32,
                "NVME_PERF_POWER_LOG_HASH": "c" * 64,
                "NVME_PERF_CAMPAIGN_REGISTRY_HASH": "d" * 64,
                "NVME_PERF_HOST_MODEL": "MacTest",
                "NVME_PERF_MACOS_VERSION": "26.0", "NVME_PERF_POWER_SOURCE_START": "AC Power",
                "NVME_PERF_POWER_SOURCE_END": "AC Power", "NVME_PERF_STARTED_AT": started,
                "NVME_PERF_DESKTOP_ELAPSED_MS": "1000.0",
            })
            receipt = _build_receipt(result, hashes)
            validate_receipt(receipt, result, hashes)
            failed = _build_failure("workload-failed")
            validate_receipt(failed)
            os.environ["NVME_PERF_POWER_SOURCE_START"] = "unknown"
            os.environ["NVME_PERF_POWER_SOURCE_END"] = "unknown"
            validate_receipt(_build_failure("artifact-invalid"))
            try:
                validate_receipt(_build_failure("workload-failed"))
            except ValueError as exc:
                assert "unknown failure power source" in str(exc)
            else:
                raise AssertionError("unknown power was accepted for a workload failure")
            try:
                _build_failure("free-form reason")
            except ValueError as exc:
                assert "canonical token" in str(exc)
            else:
                raise AssertionError("free-form failure reason was accepted")
        finally:
            os.environ.clear(); os.environ.update(previous)
        raw, _ = _read_artifact(raw_path, "raw JSON"); raw["read_latency_ns"].pop()
        raw_path.write_text(json.dumps(raw, separators=(",", ":")))
        try:
            _load_artifacts(result_path, raw_path, done_path, config_path)
        except ValueError as exc:
            assert "exact raw" in str(exc) or "exactly 20480" in str(exc)
        else:
            raise AssertionError("mutated raw vector was accepted")
    print("HVF NVMe performance receipt self-test: PASS")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--result-json", type=Path); parser.add_argument("--raw-json", type=Path)
    parser.add_argument("--done-json", type=Path); parser.add_argument("--config-json", type=Path)
    parser.add_argument("--output", type=Path); parser.add_argument("--failed-reason")
    parser.add_argument("--validate", type=Path, metavar="RECEIPT"); parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    modes = sum((args.self_test, args.validate is not None, args.output is not None))
    if modes != 1:
        parser.error("choose exactly one of --output, --validate, or --self-test")
    if args.self_test:
        self_test(); return 0
    if args.validate is not None:
        receipt = _read_receipt(args.validate)
        if receipt.get("pass") is True:
            if any(value is None for value in (args.result_json, args.raw_json, args.done_json, args.config_json)):
                parser.error("passing receipt validation requires all four private artifacts")
            assert args.result_json and args.raw_json and args.done_json and args.config_json
            result, hashes = _load_artifacts(args.result_json, args.raw_json, args.done_json, args.config_json)
            validate_receipt(receipt, result, hashes)
        else:
            validate_receipt(receipt)
        print("HVF NVMe performance receipt: valid"); return 0
    else:
        assert args.output
        if args.failed_reason is not None:
            if any(value is not None for value in (args.result_json, args.raw_json, args.done_json, args.config_json)):
                parser.error("--failed-reason does not accept private artifacts")
            receipt = _build_failure(args.failed_reason)
            validate_receipt(receipt)
        else:
            if any(value is None for value in (args.result_json, args.raw_json, args.done_json, args.config_json)):
                parser.error("successful --output requires --result-json, --raw-json, --done-json, and --config-json")
            assert args.result_json and args.raw_json and args.done_json and args.config_json
            result, hashes = _load_artifacts(args.result_json, args.raw_json, args.done_json, args.config_json)
            receipt = _build_receipt(result, hashes)
        target = args.output
    if target.exists() or target.is_symlink():
        raise ValueError("receipt output must not already exist")
    with target.open("x", encoding="utf-8") as stream:
        stream.write(json.dumps(receipt, indent=2, sort_keys=True) + "\n")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, UnicodeError, ValueError) as exc:
        print(f"error: {exc}", file=sys.stderr)
        raise SystemExit(1)
