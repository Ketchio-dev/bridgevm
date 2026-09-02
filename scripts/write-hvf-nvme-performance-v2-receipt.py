#!/usr/bin/env python3
"""Fail-closed validator and receipt writer for the frozen T16 NVMe v2 A/A profile."""

from __future__ import annotations

import argparse
import copy
import datetime as dt
import hashlib
import json
import math
import os
import re
import statistics
import sys
import tempfile
from pathlib import Path
from typing import Any

SCHEMA = "bridgevm.windows-nvme-warm-seq.v2"
WORKLOAD = "windows-nvme-warm-seq-v2"
TIER = "t16-hvf-nvme-performance"
GATE_ID = "hvf-nvme-performance-diagnostic"
REGISTRY_SCHEMA = "bridgevm.t16-nvme-calibration-registry.v2"
FILE_MIB, TRANSFER_KIB, READ_PASSES, WRITE_PASSES = 2048, 128, 16, 4
FILE_BYTES, TRANSFER_BYTES = FILE_MIB * 1024 * 1024, TRANSFER_KIB * 1024
BLOCKS_PER_PASS = FILE_BYTES // TRANSFER_BYTES
READ_OPS, WRITE_OPS = BLOCKS_PER_PASS * READ_PASSES, BLOCKS_PER_PASS * WRITE_PASSES
EXPECTED_RUNS, EXPECTED_PAIRS = 48, 24
POST_CLONE_COOLDOWN_SECONDS, GUEST_SETTLE_SECONDS, WORKLOAD_SETTLE_SECONDS = 120, 120, 30
MIN_HOST_SAMPLES, HOST_SAMPLE_INTERVAL_SECONDS, MIN_HID_IDLE_SECONDS = 48, 5, 300
QUIESCENCE_SAMPLES = 30
CPU_MEDIAN_MAX, CPU_P95_MAX = 10.0, 20.0
DISK_BPS_MEDIAN_MAX, DISK_BPS_P95_MAX, QUEUE_P95_MAX = 1_048_576.0, 4_194_304.0, 0.25
MAX_ARTIFACT_BYTES = 8 * 1024 * 1024
PRECONDITION_SHA256 = "6b60d964c6165fd46dc68630f01fc7118edb6f54915799a9cb0d59f7a961037b"
FINAL_SHA256 = "9c48543aea55b3571fd028c58d9bf5159d82a5d0d7b90acd1adc06a469cce0c8"

KNOWN_CONFOUNDERS = [
    "guest-unbuffered sequential I/O uses a host-warm backing image",
    "storage and desktop timing share one end-to-end live attempt",
    "A/A calibration measures harness noise and cannot support a product performance claim",
]
CONFIG = {
    "schema": SCHEMA,
    "workload_profile": WORKLOAD,
    "pattern_id": "offset-xorshift64-v1",
    "precondition_sha256": PRECONDITION_SHA256,
    "final_sha256": FINAL_SHA256,
    "data_path": r"C:\ProgramData\BridgeVMPerf\nvme-seq-v2.bin",
    "file_mib": FILE_MIB,
    "transfer_kib": TRANSFER_KIB,
    "read_passes": READ_PASSES,
    "write_passes": WRITE_PASSES,
    "queue_depth": 1,
    "post_warmup_settle_ms": 30_000,
    "read_flags": ["FILE_FLAG_NO_BUFFERING"],
    "write_flags": ["FILE_FLAG_NO_BUFFERING", "FILE_FLAG_WRITE_THROUGH"],
    "flush_semantics": "FlushFileBuffers after precondition and each measured write pass",
    "file_attributes": ["FILE_ATTRIBUTE_NOT_CONTENT_INDEXED"],
    "read_phase_semantics": "sum of per-pass SeekStart plus ReadFile intervals; pattern and hash verification excluded",
    "verification_semantics": "full warmup and post-read verification plus full readback after every measured write pass",
    "cache_profile": "guest-unbuffered-host-warm",
}
QUIESCENCE_CONFIG = {
    "schema": "bridgevm.hvf-nvme-quiescence.v1",
    "samples": QUIESCENCE_SAMPLES,
    "interval_seconds": 1,
    "cpu_percent": {"median_max": 10, "p95_max": 20},
    "disk_bytes_per_second": {"median_max": 1_048_576, "p95_max": 4_194_304},
    "disk_queue_length": {"p95_max": 0.25},
    "post_ready_settle_seconds": 120,
    "post_sample_quiet_seconds": 15,
}


def _canonical(document: Any) -> bytes:
    return json.dumps(document, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode()


CONFIG_BYTES, QUIESCENCE_CONFIG_CANONICAL = map(_canonical, (CONFIG, QUIESCENCE_CONFIG))
CONFIG_SHA256 = hashlib.sha256(CONFIG_BYTES).hexdigest()
EXPECTED_CONFIG_SHA256 = "243fc66006b67ff702d74609b56f2c7f044331cef31c35758e294b96865b7908"
QUIESCENCE_CONFIG_BYTES = QUIESCENCE_CONFIG_CANONICAL + b"\n"
QUIESCENCE_CONFIG_SHA256 = hashlib.sha256(QUIESCENCE_CONFIG_BYTES).hexdigest()
if CONFIG_SHA256 != EXPECTED_CONFIG_SHA256:
    raise RuntimeError("embedded v2 config differs from the registered renderer")

RAW_KEYS = {"schema", "workload_profile", "nonce", "config_sha256", "read_latency_ns", "read_pass_elapsed_ns", "write_latency_ns", "flush_latency_ns"}
DONE_KEYS = {"schema", "workload_profile", "nonce", "config_sha256", "status", "raw_sha256", "result_sha256"}
LATENCY_KEYS = {"count", "p50", "p95", "p99", "max"}
RESULT_KEYS = {
    "schema", "workload_profile", "nonce", "config_sha256", "config", "status", "raw_sha256",
    "file_bytes", "transfer_bytes", "blocks_per_pass", "precondition_write_ops", "precondition_write_bytes",
    "warmup_read_ops", "warmup_read_bytes", "measured_read_ops", "measured_read_bytes",
    "post_read_verify_ops", "post_read_verify_bytes", "measured_write_ops", "measured_write_bytes",
    "write_verify_read_ops", "write_verify_read_bytes", "read_result_count",
    "write_result_count", "final_verify_read_ops", "final_verify_read_bytes", "verified_read_ops",
    "flush_calls", "post_warmup_settle_ms", "bytes_per_sector", "file_alignment_bytes", "required_alignment_bytes", "warmup_sha256",
    "post_read_sha256", "final_sha256", "read_phase_elapsed_ns", "read_service_elapsed_ns", "read_phase_mib_per_sec",
    "read_service_mib_per_sec", "read_throughput_mib_s", "read_p50_ms", "read_p95_ms", "read_p99_ms",
    "read_max_ms", "read_pass_p50_ms", "read_pass_p95_ms", "read_pass_p99_ms", "read_pass_max_ms",
    "write_phase_elapsed_ns", "write_and_flush_service_elapsed_ns",
    "write_durable_mib_per_sec", "write_and_flush_service_mib_per_sec", "write_durable_throughput_mib_s",
    "write_p50_ms", "write_p95_ms", "write_p99_ms", "write_max_ms", "flush_p50_ms", "flush_p95_ms",
    "flush_p99_ms", "flush_max_ms", "read_latency_ns", "read_pass_elapsed_ns", "write_latency_ns", "flush_latency_ns",
}
PERFORMANCE_FIELDS = (
    "read_phase_mib_per_sec", "read_service_mib_per_sec", "read_throughput_mib_s",
    "read_p50_ms", "read_p95_ms", "read_p99_ms", "read_max_ms",
    "read_pass_p50_ms", "read_pass_p95_ms", "read_pass_p99_ms", "read_pass_max_ms",
    "write_durable_mib_per_sec", "write_and_flush_service_mib_per_sec",
    "write_durable_throughput_mib_s", "write_p50_ms", "write_p95_ms", "write_p99_ms", "write_max_ms",
    "flush_p50_ms", "flush_p95_ms", "flush_p99_ms", "flush_max_ms",
)
COUNT_FIELDS = (
    "file_bytes", "transfer_bytes", "blocks_per_pass", "precondition_write_ops", "precondition_write_bytes",
    "warmup_read_ops", "warmup_read_bytes", "measured_read_ops", "measured_read_bytes", "measured_write_ops",
    "post_read_verify_ops", "post_read_verify_bytes", "measured_write_bytes", "write_verify_read_ops",
    "write_verify_read_bytes", "read_result_count", "write_result_count",
    "final_verify_read_ops", "final_verify_read_bytes", "verified_read_ops", "flush_calls", "post_warmup_settle_ms", "bytes_per_sector",
    "file_alignment_bytes", "required_alignment_bytes", "read_phase_elapsed_ns", "read_service_elapsed_ns",
    "write_phase_elapsed_ns", "write_and_flush_service_elapsed_ns",
)
HASH_FIELDS = (
    "binary_hash", "input_manifest_sha256", "image_sha256", "vars_sha256", "firmware_sha256",
    "renderer_sha256", "config_sha256", "power_log_sha256", "campaign_registry_sha256",
    "public_seed_sha256",
    "workload_script_sha256", "raw_sha256", "result_sha256", "done_sha256", "warmup_sha256", "post_read_sha256", "final_sha256",
    "environment_policy_sha256", "environment_helper_sha256", "pmset_policy_sha256", "thermal_log_sha256", "hid_log_sha256",
    "guest_quiescence_script_sha256", "guest_quiescence_config_sha256", "guest_quiescence_log_sha256",
)
ENVIRONMENT_SUMMARY_FIELDS = (
    "thermal_sample_count", "thermal_nominal_samples", "host_hid_idle_start_seconds",
    "host_hid_idle_end_seconds", "host_hid_reset_count", "guest_quiescence_sample_count",
    "guest_cpu_median_percent", "guest_cpu_p95_percent", "guest_disk_bps_median",
    "guest_disk_bps_p95", "guest_disk_queue_p95",
)
RECEIPT_KEYS = {
    "schema_version", "tier", "gate_id", "job_id", "commit", "harness_commit", *HASH_FIELDS,
    "workload_profile", "nonce", "campaign_id", "campaign_mode", "campaign_label", "campaign_order", "campaign_pair", "campaign_ordinal",
    "campaign_expected_runs", "host_model", "macos_version", "power_source", "power_source_start",
    "power_source_end", "caffeinated", "security_services_enabled", "thermal_state",
    "post_clone_cooldown_seconds", "guest_settle_seconds", *ENVIRONMENT_SUMMARY_FIELDS,
    "file_mib", "transfer_kib", "read_passes", "write_passes", "queue_depth", "smp_cpus", "ram_mib",
    "known_confounders", *COUNT_FIELDS, *PERFORMANCE_FIELDS, "desktop_elapsed_ms", "sample_count", "run_count",
    "required_run_count", "passes", "failures", "started_at", "finished_at", "outcome", "pass", "valid",
    "invalid_reason", "claim_eligible", "evidence_paths",
}
EVIDENCE_PATHS = [
    "share/nvme-result.json", "share/nvme-raw.json", "share/nvme-result.done",
    "share/nvme-workload-config.json", "power-source.log", "pmset-policy.txt", "thermal.json", "hid.json",
    "share/bv-nvme-quiescence-v2.ps1", "share/hvf-nvme-performance-v2-quiescence.json",
    "share/nvme-quiescence-result.json",
]
FAILURE_REASONS = {
    "sealed-input-invalid", "host-preflight-invalid", "host-monitor-failed", "host-environment-invalid",
    "power-source-invalid", "guest-unreachable", "guest-quiescence-invalid", "workload-input-invalid",
    "workload-failed", "workload-timeout", "artifact-missing", "artifact-invalid", "sealed-input-changed",
    "worker-interrupted", "campaign-aborted-after-invalid-lane",
}


def _required(name: str) -> str:
    value = os.environ.get(name, "")
    if not value:
        raise ValueError(f"missing receipt field {name}")
    return value


def _hex(value: Any, width: int = 64) -> bool:
    return isinstance(value, str) and bool(re.fullmatch(rf"[0-9a-f]{{{width}}}", value))


def _positive_integer(value: Any, name: str) -> int:
    if type(value) is not int or value < 1:
        raise ValueError(f"{name} must be a positive integer")
    return value


def _finite(value: Any, name: str, *, nonnegative: bool = False) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(float(value)):
        raise ValueError(f"{name} must be finite numeric")
    converted = float(value)
    if converted < 0 if nonnegative else converted <= 0:
        raise ValueError(f"{name} is outside its numeric domain")
    return converted


def _timestamp(value: Any, name: str) -> dt.datetime:
    if not isinstance(value, str):
        raise ValueError(f"{name} must be an ISO-8601 timestamp")
    try:
        parsed = dt.datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError as exc:
        raise ValueError(f"{name} must be an ISO-8601 timestamp") from exc
    if parsed.tzinfo is None:
        raise ValueError(f"{name} must include a timezone")
    return parsed


def _read_bytes(path: Path, label: str) -> tuple[bytes, str]:
    if not path.is_file() or path.is_symlink():
        raise ValueError(f"{label} must be a regular, non-symlink file")
    payload = path.read_bytes()
    if not payload or len(payload) >= MAX_ARTIFACT_BYTES:
        raise ValueError(f"{label} violates the evidence size boundary")
    return payload, hashlib.sha256(payload).hexdigest()


def _read_compact_json(path: Path, label: str) -> tuple[dict[str, Any], str]:
    payload, digest = _read_bytes(path, label)
    if payload.startswith(b"\xef\xbb\xbf"):
        raise ValueError(f"{label} must not contain a BOM")
    def unique(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
        document: dict[str, Any] = {}
        for key, value in pairs:
            if key in document:
                raise ValueError(f"{label} contains duplicate JSON key {key!r}")
            document[key] = value
        return document
    try:
        text = payload.decode("utf-8")
        document = json.loads(text, object_pairs_hook=unique)
    except (UnicodeError, json.JSONDecodeError) as exc:
        raise ValueError(f"{label} is not valid UTF-8 JSON") from exc
    if not isinstance(document, dict) or text != text.strip() or "\n" in text or "\r" in text:
        raise ValueError(f"{label} must contain one compact JSON object")
    return document, digest


def _fixed_identity(document: dict[str, Any], keys: set[str], label: str) -> None:
    if set(document) != keys or document.get("schema") != SCHEMA or document.get("workload_profile") != WORKLOAD:
        raise ValueError(f"{label} has missing/unknown fields or a non-v2 identity")
    if not _hex(document.get("nonce"), 32) or document.get("config_sha256") != CONFIG_SHA256:
        raise ValueError(f"{label} has a non-canonical nonce or config identity")


def _percentile(values: list[float], quantile: float) -> float:
    ordered = sorted(values)
    return ordered[max(0, math.ceil(quantile * len(ordered)) - 1)]


def _latency_summary(values: list[int]) -> dict[str, int]:
    return {"count": len(values), "p50": int(_percentile(values, .50)), "p95": int(_percentile(values, .95)),
            "p99": int(_percentile(values, .99)), "max": max(values)}


def validate_artifacts(result: dict[str, Any], result_hash: str, raw: dict[str, Any], raw_hash: str,
                       done: dict[str, Any], done_hash: str) -> dict[str, str]:
    _fixed_identity(raw, RAW_KEYS, "raw JSON"); _fixed_identity(result, RESULT_KEYS, "result JSON")
    _fixed_identity(done, DONE_KEYS, "done JSON")
    identities = {(item["nonce"], item["config_sha256"], item["workload_profile"]) for item in (raw, result, done)}
    if len(identities) != 1 or result.get("status") != "passed" or done.get("status") != "passed":
        raise ValueError("raw/result/done identities or statuses disagree")
    if result.get("raw_sha256") != raw_hash or done.get("raw_sha256") != raw_hash or done.get("result_sha256") != result_hash:
        raise ValueError("raw/result/done byte seals disagree")
    if result.get("config") != CONFIG:
        raise ValueError("result does not embed the exact frozen v2 config")
    vectors: dict[str, list[int]] = {}
    for field, count in (("read_latency_ns", READ_OPS), ("read_pass_elapsed_ns", READ_PASSES),
                         ("write_latency_ns", WRITE_OPS), ("flush_latency_ns", WRITE_PASSES)):
        values = raw.get(field)
        if not isinstance(values, list) or len(values) != count or any(type(value) is not int or value < 1 for value in values):
            raise ValueError(f"raw {field} must contain exactly {count} positive integer samples")
        vectors[field] = values
        if result.get(field) != _latency_summary(values):
            raise ValueError(f"result {field} does not exactly summarize the raw samples")
    fixed = {
        "file_bytes": FILE_BYTES, "transfer_bytes": TRANSFER_BYTES, "blocks_per_pass": BLOCKS_PER_PASS,
        "precondition_write_ops": BLOCKS_PER_PASS, "precondition_write_bytes": FILE_BYTES,
        "warmup_read_ops": BLOCKS_PER_PASS, "warmup_read_bytes": FILE_BYTES,
        "measured_read_ops": READ_OPS, "measured_read_bytes": FILE_BYTES * READ_PASSES,
        "post_read_verify_ops": BLOCKS_PER_PASS, "post_read_verify_bytes": FILE_BYTES,
        "measured_write_ops": WRITE_OPS, "measured_write_bytes": FILE_BYTES * WRITE_PASSES,
        "write_verify_read_ops": WRITE_OPS, "write_verify_read_bytes": FILE_BYTES * WRITE_PASSES,
        "read_result_count": READ_PASSES, "write_result_count": WRITE_PASSES,
        "final_verify_read_ops": BLOCKS_PER_PASS, "final_verify_read_bytes": FILE_BYTES,
        "verified_read_ops": BLOCKS_PER_PASS * (2 + WRITE_PASSES), "flush_calls": WRITE_PASSES + 1,
        "post_warmup_settle_ms": 30_000,
    }
    for field, expected in fixed.items():
        if type(result.get(field)) is not int or result[field] != expected:
            raise ValueError(f"result has invalid fixed v2 count {field}")
    for field in ("bytes_per_sector", "file_alignment_bytes", "required_alignment_bytes"):
        value = _positive_integer(result.get(field), field)
        if value & (value - 1):
            raise ValueError(f"{field} must be a power of two")
    if result["required_alignment_bytes"] != max(result["bytes_per_sector"], result["file_alignment_bytes"]) or TRANSFER_BYTES % result["required_alignment_bytes"]:
        raise ValueError("storage alignment is inconsistent")
    for field in ("warmup_sha256", "post_read_sha256", "final_sha256"):
        if not _hex(result.get(field)):
            raise ValueError(f"{field} is not canonical")
    if result["warmup_sha256"] != PRECONDITION_SHA256 \
            or result["post_read_sha256"] != PRECONDITION_SHA256:
        raise ValueError("warmup/post-read verification does not match the fixed precondition pattern")
    if result["final_sha256"] != FINAL_SHA256:
        raise ValueError("final verification does not match the fixed last-write pattern")
    for field in ("read_phase_elapsed_ns", "read_service_elapsed_ns", "write_phase_elapsed_ns", "write_and_flush_service_elapsed_ns"):
        _positive_integer(result.get(field), field)
    if result["read_service_elapsed_ns"] != sum(vectors["read_latency_ns"]):
        raise ValueError("read service time does not equal the sealed samples")
    if result["read_phase_elapsed_ns"] != sum(vectors["read_pass_elapsed_ns"]):
        raise ValueError("read phase time does not equal the 16 sealed per-pass intervals")
    if result["write_and_flush_service_elapsed_ns"] != sum(vectors["write_latency_ns"]) + sum(vectors["flush_latency_ns"]):
        raise ValueError("write/flush service time does not equal the sealed samples")
    if result["read_phase_elapsed_ns"] < result["read_service_elapsed_ns"] or result["write_phase_elapsed_ns"] < result["write_and_flush_service_elapsed_ns"]:
        raise ValueError("phase time is shorter than service time")
    read_mib, write_mib = FILE_MIB * READ_PASSES, FILE_MIB * WRITE_PASSES
    rates = {
        "read_phase_mib_per_sec": read_mib / (result["read_phase_elapsed_ns"] / 1e9),
        "read_service_mib_per_sec": read_mib / (result["read_service_elapsed_ns"] / 1e9),
        "write_durable_mib_per_sec": write_mib / (result["write_phase_elapsed_ns"] / 1e9),
        "write_and_flush_service_mib_per_sec": write_mib / (result["write_and_flush_service_elapsed_ns"] / 1e9),
    }
    for field, expected in rates.items():
        if not math.isclose(_finite(result.get(field), field), expected, rel_tol=1e-12, abs_tol=1e-12):
            raise ValueError(f"{field} is inconsistent with bytes and elapsed time")
    for alias, source in (("read_throughput_mib_s", "read_phase_mib_per_sec"),
                          ("write_durable_throughput_mib_s", "write_durable_mib_per_sec")):
        if _finite(result.get(alias), alias) != result[source]:
            raise ValueError(f"{alias} disagrees with its canonical metric")
    for prefix in ("read", "read_pass", "write", "flush"):
        summary_field = "read_pass_elapsed_ns" if prefix == "read_pass" else f"{prefix}_latency_ns"
        for percentile in ("p50", "p95", "p99", "max"):
            field = f"{prefix}_{percentile}_ms"
            if _finite(result.get(field), field) != result[summary_field][percentile] / 1e6:
                raise ValueError(f"{field} disagrees with sealed latency samples")
    return {"raw_sha256": raw_hash, "result_sha256": result_hash, "done_sha256": done_hash}


def load_artifacts(result_path: Path, raw_path: Path, done_path: Path, config_path: Path) -> tuple[dict[str, Any], dict[str, str]]:
    config_payload, digest = _read_bytes(config_path, "config JSON")
    if config_payload != CONFIG_BYTES or digest != CONFIG_SHA256:
        raise ValueError("config JSON is not the exact canonical v2 config")
    result, result_hash = _read_compact_json(result_path, "result JSON")
    raw, raw_hash = _read_compact_json(raw_path, "raw JSON")
    done, done_hash = _read_compact_json(done_path, "done JSON")
    return result, validate_artifacts(result, result_hash, raw, raw_hash, done, done_hash)


def _check_exact_file(path: Path, label: str, expected: bytes) -> str:
    payload, digest = _read_bytes(path, label)
    if payload != expected:
        raise ValueError(f"{label} is not the exact registered v2 document")
    return digest


def _json_evidence(path: Path, label: str) -> tuple[dict[str, Any], str]:
    payload, digest = _read_bytes(path, label)
    if payload.startswith(b"\xef\xbb\xbf") or b"\r" in payload or payload.count(b"\n") > 1 \
            or (b"\n" in payload and not payload.endswith(b"\n")):
        raise ValueError(f"{label} has a forbidden encoding or line shape")
    def unique(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
        document: dict[str, Any] = {}
        for key, value in pairs:
            if key in document: raise ValueError(f"{label} contains a duplicate JSON key")
            document[key] = value
        return document
    try: document = json.loads(payload.decode("utf-8"), object_pairs_hook=unique)
    except (UnicodeError, json.JSONDecodeError) as exc: raise ValueError(f"{label} is not UTF-8 JSON") from exc
    if not isinstance(document, dict): raise ValueError(f"{label} is not one JSON object")
    return document, digest


def load_environment(power_path: Path, policy_path: Path, helper_path: Path, pmset_path: Path, thermal_path: Path,
                     hid_path: Path, script_path: Path, quiescence_config_path: Path, quiescence_log_path: Path,
                     nonce: str) -> tuple[dict[str, str], dict[str, Any]]:
    power, power_hash = _read_bytes(power_path, "power log")
    try: power_lines = power.decode("utf-8").splitlines()
    except UnicodeError as exc: raise ValueError("power log is not UTF-8") from exc
    drawing = [line for line in power_lines if line.startswith("Now drawing from '")]
    if not drawing or any(line != "Now drawing from 'AC Power'" for line in drawing):
        raise ValueError("power log does not prove uninterrupted AC power")
    _, policy_hash = _read_bytes(policy_path, "environment policy")
    _, helper_hash = _read_bytes(helper_path, "environment helper")
    pmset_payload, pmset_hash = _read_bytes(pmset_path, "pmset policy")
    if pmset_payload.startswith(b"\xef\xbb\xbf") or not pmset_payload.strip():
        raise ValueError("pmset policy is empty or has a forbidden encoding")
    thermal, thermal_hash = _json_evidence(thermal_path, "thermal log")
    hid, hid_hash = _json_evidence(hid_path, "HID log")
    exact_host_keys = {"schema", "sample_interval_seconds", "samples"}
    if set(thermal) != exact_host_keys or thermal.get("schema") != "bridgevm.hvf-nvme-host-thermal.v2" \
            or thermal.get("sample_interval_seconds") != HOST_SAMPLE_INTERVAL_SECONDS:
        raise ValueError("thermal log identity is invalid")
    if set(hid) != exact_host_keys or hid.get("schema") != "bridgevm.hvf-nvme-host-hid.v2" \
            or hid.get("sample_interval_seconds") != HOST_SAMPLE_INTERVAL_SECONDS:
        raise ValueError("HID log identity is invalid")
    samples, hid_samples = thermal.get("samples"), hid.get("samples")
    if not isinstance(samples, list) or not isinstance(hid_samples, list) or len(samples) != len(hid_samples) \
            or len(samples) < MIN_HOST_SAMPLES:
        raise ValueError(f"thermal/HID logs must have equal counts of at least {MIN_HOST_SAMPLES}")
    hid_values: list[int] = []
    for ordinal, (thermal_sample, hid_sample) in enumerate(zip(samples, hid_samples), 1):
        if thermal_sample != {"ordinal": ordinal, "thermal_state": 0}:
            raise ValueError("every ordered thermal sample must be nominal state 0")
        if not isinstance(hid_sample, dict) or set(hid_sample) != {"ordinal", "idle_nanoseconds"} \
                or hid_sample.get("ordinal") != ordinal or type(hid_sample.get("idle_nanoseconds")) is not int \
                or hid_sample["idle_nanoseconds"] < 0:
            raise ValueError("HID samples are malformed")
        hid_values.append(hid_sample["idle_nanoseconds"])
    resets = sum(current < previous for previous, current in zip(hid_values, hid_values[1:]))
    if hid_values[0] < MIN_HID_IDLE_SECONDS * 1_000_000_000 or resets:
        raise ValueError("HID log does not prove an initial 300-second idle window with zero resets")
    script_payload, script_hash = _read_bytes(script_path, "guest quiescence script")
    if script_payload.startswith(b"\xef\xbb\xbf") or b"\x00" in script_payload:
        raise ValueError("guest quiescence script has a forbidden encoding")
    quiescence_config_hash = _check_exact_file(quiescence_config_path, "guest quiescence config", QUIESCENCE_CONFIG_BYTES)
    quiet, quiet_hash = _json_evidence(quiescence_log_path, "guest quiescence log")
    if set(quiet) != {"schema", "nonce", "config_sha256", "security_services_enabled", "samples"} \
            or quiet.get("schema") != "bridgevm.hvf-nvme-quiescence-result.v1" or quiet.get("nonce") != nonce \
            or quiet.get("config_sha256") != quiescence_config_hash or quiet.get("security_services_enabled") is not True:
        raise ValueError("guest quiescence log identity is invalid")
    quiet_samples = quiet.get("samples")
    if not isinstance(quiet_samples, list) or len(quiet_samples) != QUIESCENCE_SAMPLES:
        raise ValueError(f"guest quiescence log must contain exactly {QUIESCENCE_SAMPLES} samples")
    cpu: list[float] = []; disk: list[float] = []; queue: list[float] = []
    for ordinal, sample in enumerate(quiet_samples, 1):
        if not isinstance(sample, dict) or set(sample) != {"ordinal", "cpu_percent", "disk_bytes_per_second", "disk_queue_length"} or sample.get("ordinal") != ordinal:
            raise ValueError("guest quiescence samples must have exact ordered fields")
        cpu_value = _finite(sample.get("cpu_percent"), "cpu_percent", nonnegative=True)
        if cpu_value > 100: raise ValueError("guest quiescence CPU sample exceeds 100 percent")
        cpu.append(cpu_value)
        disk.append(_finite(sample.get("disk_bytes_per_second"), "disk_bytes_per_second", nonnegative=True))
        queue.append(_finite(sample.get("disk_queue_length"), "disk_queue_length", nonnegative=True))
    summary = {
        "thermal_sample_count": len(samples), "thermal_nominal_samples": len(samples),
        "host_hid_idle_start_seconds": round(hid_values[0] / 1_000_000_000, 6),
        "host_hid_idle_end_seconds": round(hid_values[-1] / 1_000_000_000, 6),
        "host_hid_reset_count": resets, "guest_quiescence_sample_count": len(quiet_samples),
        "guest_cpu_median_percent": statistics.median(cpu), "guest_cpu_p95_percent": float(_percentile(cpu, .95)),
        "guest_disk_bps_median": statistics.median(disk), "guest_disk_bps_p95": float(_percentile(disk, .95)),
        "guest_disk_queue_p95": float(_percentile(queue, .95)), "security_services_enabled": True,
    }
    if summary["guest_cpu_median_percent"] > CPU_MEDIAN_MAX or summary["guest_cpu_p95_percent"] > CPU_P95_MAX \
            or summary["guest_disk_bps_median"] > DISK_BPS_MEDIAN_MAX or summary["guest_disk_bps_p95"] > DISK_BPS_P95_MAX \
            or summary["guest_disk_queue_p95"] > QUEUE_P95_MAX:
        raise ValueError("guest quiescence p95 exceeds the pre-registered ceiling")
    hashes = {
        "power_log_sha256": power_hash, "environment_policy_sha256": policy_hash, "environment_helper_sha256": helper_hash,
        "pmset_policy_sha256": pmset_hash, "thermal_log_sha256": thermal_hash, "hid_log_sha256": hid_hash,
        "guest_quiescence_script_sha256": script_hash, "guest_quiescence_config_sha256": quiescence_config_hash,
        "guest_quiescence_log_sha256": quiet_hash,
    }
    return hashes, summary


def _campaign_shape(campaign_id: Any, ordinal: Any, expected: Any, mode: Any, label: Any,
                    order: Any, pair: Any, nonce: Any) -> None:
    if not _hex(campaign_id, 32) or type(ordinal) is not int or type(expected) is not int or expected != EXPECTED_RUNS \
            or ordinal not in range(1, EXPECTED_RUNS + 1) or mode != "AA":
        raise ValueError("v2 requires one canonical exact 48-run A/A campaign")
    expected_pair = (ordinal + 1) // 2
    if type(pair) is not int or pair != expected_pair or order not in {"AB", "BA"} \
            or label != order[0 if ordinal % 2 else 1]:
        raise ValueError("campaign pair/order/label violates the counterbalanced schedule")
    if not _hex(nonce, 32):
        raise ValueError("nonce is not canonical v2 lane identity")


def validate_receipt(receipt: dict[str, Any], result: dict[str, Any] | None = None,
                     hashes: dict[str, str] | None = None, environment: dict[str, Any] | None = None) -> None:
    if set(receipt) != RECEIPT_KEYS or receipt.get("schema_version") != 2 or receipt.get("workload_profile") != WORKLOAD:
        raise ValueError("receipt is not an exact v2 receipt")
    if receipt.get("tier") != TIER or receipt.get("gate_id") != GATE_ID:
        raise ValueError("receipt tier or gate is invalid")
    if receipt.get("file_mib") != FILE_MIB or receipt.get("transfer_kib") != TRANSFER_KIB \
            or receipt.get("read_passes") != READ_PASSES or receipt.get("write_passes") != WRITE_PASSES \
            or receipt.get("queue_depth") != 1:
        raise ValueError("receipt geometry is not frozen v2 geometry")
    if receipt.get("claim_eligible") is not False or receipt.get("known_confounders") != KNOWN_CONFOUNDERS:
        raise ValueError("receipt claim/confounder state is invalid")
    if receipt.get("pass") is False:
        if receipt.get("valid") is not False or receipt.get("outcome") != "failed" \
                or receipt.get("invalid_reason") not in FAILURE_REASONS or receipt.get("evidence_paths") not in ([],):
            raise ValueError("failed receipt outcome is not canonical")
        if any(receipt.get(field) is not None for field in PERFORMANCE_FIELDS + ("desktop_elapsed_ms", "nonce",
                "raw_sha256", "result_sha256", "done_sha256", "warmup_sha256", "post_read_sha256", "final_sha256")):
            raise ValueError("failed receipt publishes incomplete result evidence")
        if any(receipt.get(field) != 0 for field in COUNT_FIELDS + ("sample_count", "run_count", "passes")) \
                or receipt.get("required_run_count") != 1 or receipt.get("failures") != 1:
            raise ValueError("failed receipt counts are not canonical")
        for field in HASH_FIELDS:
            if receipt.get(field) is not None and not _hex(receipt[field]):
                raise ValueError(f"failed receipt {field} is neither null nor canonical")
        if not _hex(receipt.get("commit"), 40) or receipt.get("harness_commit") != receipt.get("commit") \
                or not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]{0,127}", str(receipt.get("job_id", ""))):
            raise ValueError("failed receipt source identity is invalid")
        if _timestamp(receipt.get("finished_at"), "finished_at") <= _timestamp(receipt.get("started_at"), "started_at"):
            raise ValueError("failed receipt timestamps are not increasing")
        return
    _campaign_shape(receipt.get("campaign_id"), receipt.get("campaign_ordinal"), receipt.get("campaign_expected_runs"),
                    receipt.get("campaign_mode"), receipt.get("campaign_label"), receipt.get("campaign_order"),
                    receipt.get("campaign_pair"), receipt.get("nonce"))
    if receipt.get("guest_quiescence_config_sha256") != QUIESCENCE_CONFIG_SHA256:
        raise ValueError("receipt does not seal the registered environment/quiescence policy")
    if any(not _hex(receipt.get(field)) for field in HASH_FIELDS):
        raise ValueError("receipt has a non-canonical hash")
    if receipt.get("config_sha256") != CONFIG_SHA256 or receipt.get("power_source") != "AC Power" \
            or receipt.get("power_source_start") != "AC Power" or receipt.get("power_source_end") != "AC Power" \
            or receipt.get("caffeinated") is not True or receipt.get("security_services_enabled") is not True \
            or receipt.get("thermal_state") != "nominal" or receipt.get("post_clone_cooldown_seconds") != POST_CLONE_COOLDOWN_SECONDS \
            or receipt.get("guest_settle_seconds") != GUEST_SETTLE_SECONDS:
        raise ValueError("receipt environment validation values are invalid")
    if receipt.get("smp_cpus") != 4 or receipt.get("ram_mib") != 6144 \
            or not isinstance(receipt.get("host_model"), str) or receipt["host_model"].strip().lower() in {"", "unknown"} \
            or not isinstance(receipt.get("macos_version"), str) or receipt["macos_version"].strip().lower() in {"", "unknown"}:
        raise ValueError("receipt host/VM identity is not the fixed live profile")
    if type(receipt.get("thermal_sample_count")) is not int or receipt["thermal_sample_count"] < MIN_HOST_SAMPLES \
            or receipt.get("thermal_nominal_samples") != receipt.get("thermal_sample_count") \
            or _finite(receipt.get("host_hid_idle_start_seconds"), "host_hid_idle_start_seconds") < MIN_HID_IDLE_SECONDS \
            or _finite(receipt.get("host_hid_idle_end_seconds"), "host_hid_idle_end_seconds") < receipt["host_hid_idle_start_seconds"] \
            or receipt.get("host_hid_reset_count") != 0 \
            or receipt.get("guest_quiescence_sample_count") != QUIESCENCE_SAMPLES:
        raise ValueError("receipt environment sample/idle counts are invalid")
    for field in ENVIRONMENT_SUMMARY_FIELDS[5:]:
        _finite(receipt.get(field), field, nonnegative=True)
    if receipt["guest_cpu_median_percent"] > CPU_MEDIAN_MAX or receipt["guest_cpu_p95_percent"] > CPU_P95_MAX \
            or receipt["guest_disk_bps_median"] > DISK_BPS_MEDIAN_MAX or receipt["guest_disk_bps_p95"] > DISK_BPS_P95_MAX \
            or receipt["guest_disk_queue_p95"] > QUEUE_P95_MAX:
        raise ValueError("receipt quiescence validation values exceed policy")
    if not _hex(receipt.get("commit"), 40) or receipt.get("harness_commit") != receipt.get("commit") \
            or not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]{0,127}", str(receipt.get("job_id", ""))):
        raise ValueError("receipt source/job identity is invalid")
    started = _timestamp(receipt.get("started_at"), "started_at"); finished = _timestamp(receipt.get("finished_at"), "finished_at")
    if finished <= started:
        raise ValueError("receipt timestamps are not increasing")
    if receipt.get("evidence_paths") != EVIDENCE_PATHS:
        raise ValueError("receipt evidence description is not exact")
    if any(receipt.get(field) != 1 for field in ("sample_count", "run_count", "required_run_count", "passes")) \
            or receipt.get("failures") != 0 or receipt.get("outcome") != "completed" or receipt.get("pass") is not True \
            or receipt.get("valid") is not True or receipt.get("invalid_reason") != "" or receipt.get("claim_eligible") is not False:
        raise ValueError("receipt success/claim state is invalid")
    for field in COUNT_FIELDS:
        _positive_integer(receipt.get(field), field)
    for field in PERFORMANCE_FIELDS + ("desktop_elapsed_ms",):
        _finite(receipt.get(field), field)
    if result is not None:
        for field in COUNT_FIELDS + PERFORMANCE_FIELDS + ("warmup_sha256", "post_read_sha256", "final_sha256", "nonce"):
            if receipt.get(field) != result.get(field):
                raise ValueError(f"receipt {field} disagrees with the authenticated result")
    if hashes is not None:
        for field, value in hashes.items():
            if receipt.get(field) != value:
                raise ValueError(f"receipt {field} disagrees with authenticated bytes")
    if environment is not None:
        for field, value in environment.items():
            if receipt.get(field) != value:
                raise ValueError(f"receipt {field} disagrees with host-derived environment evidence")


def _env_identity(nonce: str, artifact_hashes: dict[str, str], evidence_hashes: dict[str, str], summary: dict[str, Any]) -> dict[str, Any]:
    ordinal_text = _required("NVME_PERF_CAMPAIGN_ORDINAL"); expected_text = _required("NVME_PERF_CAMPAIGN_EXPECTED_RUNS")
    if not ordinal_text.isascii() or not ordinal_text.isdigit() or not expected_text.isascii() or not expected_text.isdigit():
        raise ValueError("campaign numbers must be canonical decimal integers")
    ordinal, expected = int(ordinal_text), int(expected_text)
    campaign_id, mode = _required("NVME_PERF_CAMPAIGN_ID"), _required("NVME_PERF_CAMPAIGN_MODE")
    label, order = _required("NVME_PERF_CAMPAIGN_LABEL"), _required("NVME_PERF_CAMPAIGN_ORDER")
    pair_text = _required("NVME_PERF_CAMPAIGN_PAIR")
    if not pair_text.isascii() or not pair_text.isdigit(): raise ValueError("campaign pair must be canonical decimal")
    pair = int(pair_text); _campaign_shape(campaign_id, ordinal, expected, mode, label, order, pair, nonce)
    hash_env = {
        "binary_hash": "NVME_PERF_BINARY_HASH", "input_manifest_sha256": "NVME_PERF_MANIFEST_HASH",
        "image_sha256": "NVME_PERF_IMAGE_HASH", "vars_sha256": "NVME_PERF_VARS_HASH",
        "firmware_sha256": "NVME_PERF_FIRMWARE_HASH", "renderer_sha256": "NVME_PERF_RENDERER_HASH",
        "config_sha256": "NVME_PERF_CONFIG_HASH", "campaign_registry_sha256": "NVME_PERF_CAMPAIGN_REGISTRY_HASH",
        "public_seed_sha256": "NVME_PERF_PUBLIC_SEED_HASH",
        "workload_script_sha256": "NVME_PERF_WORKLOAD_SCRIPT_HASH",
        "environment_policy_sha256": "NVME_PERF_ENVIRONMENT_POLICY_HASH",
        "environment_helper_sha256": "NVME_PERF_ENVIRONMENT_HELPER_HASH",
        "pmset_policy_sha256": "NVME_PERF_PMSET_POLICY_HASH", "thermal_log_sha256": "NVME_PERF_THERMAL_LOG_HASH",
        "hid_log_sha256": "NVME_PERF_HID_LOG_HASH", "guest_quiescence_script_sha256": "NVME_PERF_GUEST_QUIESCENCE_SCRIPT_HASH",
        "guest_quiescence_config_sha256": "NVME_PERF_GUEST_QUIESCENCE_CONFIG_HASH",
        "guest_quiescence_log_sha256": "NVME_PERF_GUEST_QUIESCENCE_LOG_HASH", "power_log_sha256": "NVME_PERF_POWER_LOG_HASH",
    }
    identity = {field: _required(env) for field, env in hash_env.items()}
    identity.update(artifact_hashes); identity.update(evidence_hashes)
    if any(not _hex(identity[field]) for field in hash_env):
        raise ValueError("sealed environment contains a non-canonical hash")
    for field, env in hash_env.items():
        if field in artifact_hashes or field in evidence_hashes:
            if _required(env) != identity[field]:
                raise ValueError(f"{env} disagrees with the actual evidence bytes")
    if identity["config_sha256"] != CONFIG_SHA256:
        raise ValueError("sealed config is not v2")
    summary_env = {
        "thermal_sample_count": "NVME_PERF_THERMAL_SAMPLE_COUNT", "thermal_nominal_samples": "NVME_PERF_THERMAL_NOMINAL_SAMPLES",
        "host_hid_idle_start_seconds": "NVME_PERF_HOST_HID_IDLE_START_SECONDS", "host_hid_idle_end_seconds": "NVME_PERF_HOST_HID_IDLE_END_SECONDS",
        "host_hid_reset_count": "NVME_PERF_HOST_HID_RESET_COUNT", "guest_quiescence_sample_count": "NVME_PERF_GUEST_QUIESCENCE_SAMPLE_COUNT",
        "guest_cpu_median_percent": "NVME_PERF_GUEST_CPU_MEDIAN_PERCENT",
        "guest_cpu_p95_percent": "NVME_PERF_GUEST_CPU_P95_PERCENT",
        "guest_disk_bps_median": "NVME_PERF_GUEST_DISK_BPS_MEDIAN",
        "guest_disk_bps_p95": "NVME_PERF_GUEST_DISK_BPS_P95",
        "guest_disk_queue_p95": "NVME_PERF_GUEST_DISK_QUEUE_P95",
    }
    integer_summary = {"thermal_sample_count", "thermal_nominal_samples", "host_hid_reset_count",
                       "guest_quiescence_sample_count"}
    for field, env in summary_env.items():
        raw = _required(env); parsed: Any = int(raw) if field in integer_summary else float(raw)
        if parsed != summary[field]:
            raise ValueError(f"{env} disagrees with host-derived evidence")
    job_id, commit = _required("NVME_PERF_JOB_ID"), _required("NVME_PERF_HARNESS_COMMIT")
    if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]{0,127}", job_id) or not _hex(commit, 40):
        raise ValueError("job or harness identity is not canonical")
    if _required("NVME_PERF_POWER_SOURCE_START") != "AC Power" or _required("NVME_PERF_POWER_SOURCE_END") != "AC Power":
        raise ValueError("v2 measurements require AC Power throughout")
    if _required("NVME_PERF_CAFFEINATED") != "true" or _required("NVME_PERF_SECURITY_SERVICES_ENABLED") != "true":
        raise ValueError("caffeination and security services must remain enabled")
    started = _required("NVME_PERF_STARTED_AT"); _timestamp(started, "NVME_PERF_STARTED_AT")
    finished = dt.datetime.now(dt.timezone.utc).isoformat().replace("+00:00", "Z")
    if _timestamp(finished, "finished_at") <= _timestamp(started, "started_at"):
        raise ValueError("measurement start must precede receipt creation")
    identity.update(summary)
    identity.update({"job_id": job_id, "commit": commit, "harness_commit": commit,
                     "campaign_id": campaign_id, "campaign_mode": mode, "campaign_label": label,
                     "campaign_order": order, "campaign_pair": pair, "campaign_ordinal": ordinal,
                     "campaign_expected_runs": expected, "host_model": _required("NVME_PERF_HOST_MODEL"),
                     "macos_version": _required("NVME_PERF_MACOS_VERSION"), "smp_cpus": 4, "ram_mib": 6144,
                     "desktop_elapsed_ms": float(_required("NVME_PERF_DESKTOP_ELAPSED_MS")),
                     "started_at": started, "finished_at": finished})
    return identity


def build_receipt(identity: dict[str, Any], result: dict[str, Any], hashes: dict[str, str],
                  evidence_hashes: dict[str, str], environment: dict[str, Any]) -> dict[str, Any]:
    receipt = {
        "schema_version": 2, "tier": TIER, "gate_id": GATE_ID, **identity, **hashes, **evidence_hashes, **environment,
        "workload_profile": WORKLOAD, "nonce": result["nonce"], "power_source": "AC Power",
        "power_source_start": "AC Power", "power_source_end": "AC Power", "caffeinated": True,
        "security_services_enabled": True, "thermal_state": "nominal",
        "post_clone_cooldown_seconds": POST_CLONE_COOLDOWN_SECONDS, "guest_settle_seconds": GUEST_SETTLE_SECONDS,
        "file_mib": FILE_MIB, "transfer_kib": TRANSFER_KIB, "read_passes": READ_PASSES,
        "write_passes": WRITE_PASSES, "queue_depth": 1, "known_confounders": KNOWN_CONFOUNDERS,
        "sample_count": 1, "run_count": 1, "required_run_count": 1, "passes": 1, "failures": 0,
        "outcome": "completed", "pass": True, "valid": True, "invalid_reason": "", "claim_eligible": False,
        "evidence_paths": EVIDENCE_PATHS,
    }
    for field in COUNT_FIELDS + PERFORMANCE_FIELDS + ("warmup_sha256", "post_read_sha256", "final_sha256"):
        receipt[field] = result[field]
    validate_receipt(receipt, result, {**hashes, **evidence_hashes}, environment)
    return receipt


def build_failure(reason: str, actual_paths: dict[str, Path | None]) -> dict[str, Any]:
    if reason not in FAILURE_REASONS:
        raise ValueError("failure reason is not an allowed canonical token")
    env_names = {
        "binary_hash": "NVME_PERF_BINARY_HASH", "input_manifest_sha256": "NVME_PERF_MANIFEST_HASH",
        "image_sha256": "NVME_PERF_IMAGE_HASH", "vars_sha256": "NVME_PERF_VARS_HASH",
        "firmware_sha256": "NVME_PERF_FIRMWARE_HASH", "renderer_sha256": "NVME_PERF_RENDERER_HASH",
        "config_sha256": "NVME_PERF_CONFIG_HASH", "campaign_registry_sha256": "NVME_PERF_CAMPAIGN_REGISTRY_HASH",
        "public_seed_sha256": "NVME_PERF_PUBLIC_SEED_HASH",
        "workload_script_sha256": "NVME_PERF_WORKLOAD_SCRIPT_HASH", "power_log_sha256": "NVME_PERF_POWER_LOG_HASH",
        "environment_policy_sha256": "NVME_PERF_ENVIRONMENT_POLICY_HASH",
        "environment_helper_sha256": "NVME_PERF_ENVIRONMENT_HELPER_HASH",
        "pmset_policy_sha256": "NVME_PERF_PMSET_POLICY_HASH", "thermal_log_sha256": "NVME_PERF_THERMAL_LOG_HASH",
        "hid_log_sha256": "NVME_PERF_HID_LOG_HASH", "guest_quiescence_script_sha256": "NVME_PERF_GUEST_QUIESCENCE_SCRIPT_HASH",
        "guest_quiescence_config_sha256": "NVME_PERF_GUEST_QUIESCENCE_CONFIG_HASH",
        "guest_quiescence_log_sha256": "NVME_PERF_GUEST_QUIESCENCE_LOG_HASH",
    }
    sealed: dict[str, str | None] = {}
    for field, env in env_names.items():
        value = os.environ.get(env, ""); sealed[field] = value if _hex(value) else None
    for field, path in actual_paths.items():
        if path is None: continue
        _, digest = _read_bytes(path, field)
        if sealed[field] is not None and sealed[field] != digest:
            raise ValueError(f"{env_names[field]} disagrees with available failure evidence")
        sealed[field] = digest
    for field in ("raw_sha256", "result_sha256", "done_sha256", "warmup_sha256", "post_read_sha256", "final_sha256"):
        sealed[field] = None
    commit = _required("NVME_PERF_HARNESS_COMMIT")
    if not _hex(commit, 40): raise ValueError("failure harness commit is not canonical")
    started = _required("NVME_PERF_STARTED_AT"); _timestamp(started, "NVME_PERF_STARTED_AT")
    finished = dt.datetime.now(dt.timezone.utc).isoformat().replace("+00:00", "Z")
    counts = {field: 0 for field in COUNT_FIELDS}; metrics = {field: None for field in PERFORMANCE_FIELDS}
    receipt: dict[str, Any] = {
        "schema_version": 2, "tier": TIER, "gate_id": GATE_ID, "job_id": _required("NVME_PERF_JOB_ID"),
        "commit": commit, "harness_commit": commit, **sealed, "workload_profile": WORKLOAD, "nonce": None,
        "campaign_id": None, "campaign_mode": None, "campaign_label": None, "campaign_order": None,
        "campaign_pair": None, "campaign_ordinal": None, "campaign_expected_runs": None,
        "host_model": os.environ.get("NVME_PERF_HOST_MODEL", "unknown"),
        "macos_version": os.environ.get("NVME_PERF_MACOS_VERSION", "unknown"),
        "power_source": os.environ.get("NVME_PERF_POWER_SOURCE_START", "unknown"),
        "power_source_start": os.environ.get("NVME_PERF_POWER_SOURCE_START", "unknown"),
        "power_source_end": os.environ.get("NVME_PERF_POWER_SOURCE_END", "unknown"),
        "caffeinated": os.environ.get("NVME_PERF_CAFFEINATED") == "true", "security_services_enabled": False,
        "thermal_state": "unknown", "post_clone_cooldown_seconds": POST_CLONE_COOLDOWN_SECONDS,
        "guest_settle_seconds": GUEST_SETTLE_SECONDS, "thermal_sample_count": 0, "thermal_nominal_samples": 0,
        "host_hid_idle_start_seconds": 0, "host_hid_idle_end_seconds": 0, "host_hid_reset_count": 0,
        "guest_quiescence_sample_count": 0, "guest_cpu_median_percent": None, "guest_cpu_p95_percent": None,
        "guest_disk_bps_median": None, "guest_disk_bps_p95": None, "guest_disk_queue_p95": None,
        "file_mib": FILE_MIB, "transfer_kib": TRANSFER_KIB, "read_passes": READ_PASSES,
        "write_passes": WRITE_PASSES, "queue_depth": 1, "smp_cpus": 4, "ram_mib": 6144,
        "known_confounders": KNOWN_CONFOUNDERS, **counts, **metrics, "desktop_elapsed_ms": None,
        "sample_count": 0, "run_count": 0, "required_run_count": 1, "passes": 0, "failures": 1,
        "started_at": started, "finished_at": finished, "outcome": "failed", "pass": False, "valid": False,
        "invalid_reason": reason, "claim_eligible": False, "evidence_paths": [],
    }
    validate_receipt(receipt); return receipt


def _fixture(directory: Path, *, campaign_id: str = "0" * 32, ordinal: int = 1,
             read_phase_ns: int = 32_000_000_000, nonce_override: str | None = None) -> tuple[dict[str, Any], dict[str, str], dict[str, str], dict[str, Any]]:
    directory.mkdir(parents=True, exist_ok=True)
    nonce = nonce_override or hashlib.sha256(f"bridgevm:t16:v2:{campaign_id}:{ordinal}".encode()).hexdigest()[:32]
    if not _hex(nonce, 32): raise ValueError("fixture nonce must be canonical")
    pass_elapsed = [read_phase_ns // READ_PASSES] * READ_PASSES
    pass_elapsed[-1] += read_phase_ns - sum(pass_elapsed)
    raw = {"schema": SCHEMA, "workload_profile": WORKLOAD, "nonce": nonce, "config_sha256": CONFIG_SHA256,
           "read_latency_ns": [1] * READ_OPS, "read_pass_elapsed_ns": pass_elapsed,
           "write_latency_ns": [1] * WRITE_OPS, "flush_latency_ns": [1] * WRITE_PASSES}
    raw_path = directory / "raw.json"; raw_path.write_bytes(_canonical(raw)); raw_hash = hashlib.sha256(raw_path.read_bytes()).hexdigest()
    result: dict[str, Any] = {"schema": SCHEMA, "workload_profile": WORKLOAD, "nonce": nonce, "config_sha256": CONFIG_SHA256,
        "config": CONFIG, "status": "passed", "raw_sha256": raw_hash, "file_bytes": FILE_BYTES,
        "transfer_bytes": TRANSFER_BYTES, "blocks_per_pass": BLOCKS_PER_PASS, "precondition_write_ops": BLOCKS_PER_PASS,
        "precondition_write_bytes": FILE_BYTES, "warmup_read_ops": BLOCKS_PER_PASS, "warmup_read_bytes": FILE_BYTES,
        "measured_read_ops": READ_OPS, "measured_read_bytes": FILE_BYTES * READ_PASSES,
        "post_read_verify_ops": BLOCKS_PER_PASS, "post_read_verify_bytes": FILE_BYTES,
        "measured_write_ops": WRITE_OPS, "measured_write_bytes": FILE_BYTES * WRITE_PASSES,
        "write_verify_read_ops": WRITE_OPS, "write_verify_read_bytes": FILE_BYTES * WRITE_PASSES,
        "read_result_count": READ_PASSES, "write_result_count": WRITE_PASSES,
        "final_verify_read_ops": BLOCKS_PER_PASS, "final_verify_read_bytes": FILE_BYTES,
        "verified_read_ops": BLOCKS_PER_PASS * (2 + WRITE_PASSES), "flush_calls": WRITE_PASSES + 1,
        "post_warmup_settle_ms": 30_000,
        "bytes_per_sector": 512, "file_alignment_bytes": 4096, "required_alignment_bytes": 4096,
        "warmup_sha256": PRECONDITION_SHA256, "post_read_sha256": PRECONDITION_SHA256,
        "final_sha256": FINAL_SHA256,
        "read_phase_elapsed_ns": read_phase_ns,
        "read_service_elapsed_ns": READ_OPS, "write_phase_elapsed_ns": 10_000_000_000,
        "write_and_flush_service_elapsed_ns": WRITE_OPS + WRITE_PASSES,
        "read_phase_mib_per_sec": FILE_MIB * READ_PASSES / (read_phase_ns / 1e9),
        "read_service_mib_per_sec": FILE_MIB * READ_PASSES / (READ_OPS / 1e9),
        "write_durable_mib_per_sec": FILE_MIB * WRITE_PASSES / 10.0,
        "write_and_flush_service_mib_per_sec": FILE_MIB * WRITE_PASSES / ((WRITE_OPS + WRITE_PASSES) / 1e9),
        "read_throughput_mib_s": FILE_MIB * READ_PASSES / (read_phase_ns / 1e9),
        "write_durable_throughput_mib_s": FILE_MIB * WRITE_PASSES / 10.0}
    for prefix, values in (("read", raw["read_latency_ns"]), ("read_pass", raw["read_pass_elapsed_ns"]),
                           ("write", raw["write_latency_ns"]), ("flush", raw["flush_latency_ns"])):
        summary_field = "read_pass_elapsed_ns" if prefix == "read_pass" else f"{prefix}_latency_ns"
        result[summary_field] = _latency_summary(values)
        for percentile in ("p50", "p95", "p99", "max"):
            result[f"{prefix}_{percentile}_ms"] = result[summary_field][percentile] / 1e6
    result_path = directory / "result.json"; result_path.write_bytes(_canonical(result)); result_hash = hashlib.sha256(result_path.read_bytes()).hexdigest()
    done = {"schema": SCHEMA, "workload_profile": WORKLOAD, "nonce": nonce, "config_sha256": CONFIG_SHA256,
            "status": "passed", "raw_sha256": raw_hash, "result_sha256": result_hash}
    done_path = directory / "done.json"; done_path.write_bytes(_canonical(done)); (directory / "config.json").write_bytes(CONFIG_BYTES)
    (directory / "power.log").write_text("Now drawing from 'AC Power'\n", encoding="utf-8")
    (directory / "environment.sh").write_bytes(b"# sealed environment policy v2\n")
    (directory / "environment.py").write_bytes(b"# sealed environment helper v2\n")
    (directory / "pmset.txt").write_bytes(b"Battery Power:\n sleep 1\nAC Power:\n sleep 0\n")
    thermal = {"schema": "bridgevm.hvf-nvme-host-thermal.v2", "sample_interval_seconds": HOST_SAMPLE_INTERVAL_SECONDS,
               "samples": [{"ordinal": i, "thermal_state": 0} for i in range(1, MIN_HOST_SAMPLES + 1)]}
    (directory / "thermal.json").write_bytes(_canonical(thermal) + b"\n")
    hid = {"schema": "bridgevm.hvf-nvme-host-hid.v2", "sample_interval_seconds": HOST_SAMPLE_INTERVAL_SECONDS,
           "samples": [{"ordinal": i, "idle_nanoseconds": 300_000_000_000 + (i - 1) * 5_000_000_000}
                       for i in range(1, MIN_HOST_SAMPLES + 1)]}
    (directory / "hid.json").write_bytes(_canonical(hid) + b"\n"); (directory / "quiet.ps1").write_bytes(b"# sealed guest quiescence probe v2\r\n")
    (directory / "quiet-config.json").write_bytes(QUIESCENCE_CONFIG_BYTES)
    quiet = {"schema": "bridgevm.hvf-nvme-quiescence-result.v1", "nonce": nonce, "config_sha256": QUIESCENCE_CONFIG_SHA256,
             "security_services_enabled": True,
             "samples": [{"ordinal": i, "cpu_percent": 1.0, "disk_bytes_per_second": 0.0, "disk_queue_length": 0.0}
                         for i in range(1, QUIESCENCE_SAMPLES + 1)]}
    (directory / "quiet.json").write_bytes(_canonical(quiet) + b"\n")
    loaded_result, hashes = load_artifacts(result_path, raw_path, done_path, directory / "config.json")
    evidence_hashes, environment = load_environment(directory / "power.log", directory / "environment.sh",
        directory / "environment.py", directory / "pmset.txt", directory / "thermal.json", directory / "hid.json", directory / "quiet.ps1",
        directory / "quiet-config.json", directory / "quiet.json", nonce)
    return loaded_result, hashes, evidence_hashes, environment


def self_test() -> None:
    tests = 0
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary); result, hashes, evidence_hashes, environment = _fixture(root)
        identity = {
            "job_id": "t16-v2-fixture-001", "commit": "1" * 40, "harness_commit": "1" * 40,
            "binary_hash": "2" * 64, "input_manifest_sha256": "3" * 64, "image_sha256": "4" * 64,
            "vars_sha256": "5" * 64, "firmware_sha256": "6" * 64, "renderer_sha256": "7" * 64,
            "config_sha256": CONFIG_SHA256, "campaign_registry_sha256": "8" * 64,
            "public_seed_sha256": "c" * 64,
            "workload_script_sha256": "9" * 64, "campaign_id": "0" * 32, "campaign_mode": "AA",
            "campaign_label": "A", "campaign_order": "AB", "campaign_pair": 1,
            "campaign_ordinal": 1, "campaign_expected_runs": EXPECTED_RUNS,
            "host_model": "MacTest", "macos_version": "26.0", "smp_cpus": 4, "ram_mib": 6144,
            "desktop_elapsed_ms": 1000.0, "started_at": "2026-01-01T00:00:00Z", "finished_at": "2026-01-01T00:01:00Z",
        }
        receipt = build_receipt(identity, result, hashes, evidence_hashes, environment); tests += 1
        previous = os.environ.copy()
        try:
            env_values = {
                "NVME_PERF_CAMPAIGN_ORDINAL": "1", "NVME_PERF_CAMPAIGN_EXPECTED_RUNS": "48",
                "NVME_PERF_CAMPAIGN_ID": "0" * 32, "NVME_PERF_CAMPAIGN_MODE": "AA",
                "NVME_PERF_CAMPAIGN_LABEL": "A", "NVME_PERF_CAMPAIGN_ORDER": "AB", "NVME_PERF_CAMPAIGN_PAIR": "1",
                "NVME_PERF_BINARY_HASH": "2" * 64, "NVME_PERF_MANIFEST_HASH": "3" * 64,
                "NVME_PERF_IMAGE_HASH": "4" * 64, "NVME_PERF_VARS_HASH": "5" * 64,
                "NVME_PERF_FIRMWARE_HASH": "6" * 64, "NVME_PERF_RENDERER_HASH": "7" * 64,
                "NVME_PERF_CONFIG_HASH": CONFIG_SHA256, "NVME_PERF_CAMPAIGN_REGISTRY_HASH": "8" * 64,
                "NVME_PERF_PUBLIC_SEED_HASH": "c" * 64, "NVME_PERF_WORKLOAD_SCRIPT_HASH": "9" * 64,
                "NVME_PERF_JOB_ID": "t16-v2-env-fixture", "NVME_PERF_HARNESS_COMMIT": "1" * 40,
                "NVME_PERF_POWER_SOURCE_START": "AC Power", "NVME_PERF_POWER_SOURCE_END": "AC Power",
                "NVME_PERF_CAFFEINATED": "true", "NVME_PERF_SECURITY_SERVICES_ENABLED": "true",
                "NVME_PERF_STARTED_AT": "2026-01-01T00:00:00Z", "NVME_PERF_HOST_MODEL": "MacTest",
                "NVME_PERF_MACOS_VERSION": "26.0", "NVME_PERF_DESKTOP_ELAPSED_MS": "1000.0",
            }
            hash_env = {
                "power_log_sha256": "NVME_PERF_POWER_LOG_HASH", "environment_policy_sha256": "NVME_PERF_ENVIRONMENT_POLICY_HASH",
                "environment_helper_sha256": "NVME_PERF_ENVIRONMENT_HELPER_HASH", "pmset_policy_sha256": "NVME_PERF_PMSET_POLICY_HASH",
                "thermal_log_sha256": "NVME_PERF_THERMAL_LOG_HASH", "hid_log_sha256": "NVME_PERF_HID_LOG_HASH",
                "guest_quiescence_script_sha256": "NVME_PERF_GUEST_QUIESCENCE_SCRIPT_HASH",
                "guest_quiescence_config_sha256": "NVME_PERF_GUEST_QUIESCENCE_CONFIG_HASH",
                "guest_quiescence_log_sha256": "NVME_PERF_GUEST_QUIESCENCE_LOG_HASH",
            }
            env_values.update({name: evidence_hashes[field] for field, name in hash_env.items()})
            summary_names = {
                "thermal_sample_count": "NVME_PERF_THERMAL_SAMPLE_COUNT", "thermal_nominal_samples": "NVME_PERF_THERMAL_NOMINAL_SAMPLES",
                "host_hid_idle_start_seconds": "NVME_PERF_HOST_HID_IDLE_START_SECONDS", "host_hid_idle_end_seconds": "NVME_PERF_HOST_HID_IDLE_END_SECONDS",
                "host_hid_reset_count": "NVME_PERF_HOST_HID_RESET_COUNT", "guest_quiescence_sample_count": "NVME_PERF_GUEST_QUIESCENCE_SAMPLE_COUNT",
                "guest_cpu_median_percent": "NVME_PERF_GUEST_CPU_MEDIAN_PERCENT", "guest_cpu_p95_percent": "NVME_PERF_GUEST_CPU_P95_PERCENT",
                "guest_disk_bps_median": "NVME_PERF_GUEST_DISK_BPS_MEDIAN", "guest_disk_bps_p95": "NVME_PERF_GUEST_DISK_BPS_P95",
                "guest_disk_queue_p95": "NVME_PERF_GUEST_DISK_QUEUE_P95",
            }
            env_values.update({name: str(environment[field]) for field, name in summary_names.items()})
            os.environ.update(env_values)
            from_env = _env_identity(result["nonce"], hashes, evidence_hashes, environment)
            assert from_env["host_hid_idle_start_seconds"] == 300.0 and from_env["host_hid_reset_count"] == 0
            tests += 1
        finally:
            os.environ.clear(); os.environ.update(previous)
        previous = os.environ.copy()
        try:
            os.environ.update({"NVME_PERF_JOB_ID": "t16-v2-failed", "NVME_PERF_HARNESS_COMMIT": "1" * 40,
                               "NVME_PERF_STARTED_AT": "2026-01-01T00:00:00Z"})
            empty_paths = {field: None for field in (
                "power_log_sha256", "environment_policy_sha256", "environment_helper_sha256", "pmset_policy_sha256",
                "thermal_log_sha256", "hid_log_sha256", "guest_quiescence_script_sha256",
                "guest_quiescence_config_sha256", "guest_quiescence_log_sha256")}
            for reason in sorted(FAILURE_REASONS):
                failed = build_failure(reason, empty_paths)
                assert failed["invalid_reason"] == reason and failed["claim_eligible"] is False and failed["pass"] is False
            try: build_failure("free-form-failure", empty_paths)
            except ValueError as exc: assert "canonical token" in str(exc)
            else: raise AssertionError("free-form failure token was accepted")
            tests += len(FAILURE_REASONS) + 1
        finally:
            os.environ.clear(); os.environ.update(previous)
        for name, mutate, message in (
            ("v1 receipt", lambda x: x.__setitem__("schema_version", 1), "exact v2"),
            ("claim", lambda x: x.__setitem__("claim_eligible", True), "claim/confounder"),
            ("geometry", lambda x: x.__setitem__("file_mib", 2047), "geometry"),
            ("AA only", lambda x: x.__setitem__("campaign_mode", "AB"), "48-run A/A"),
            ("run count", lambda x: x.__setitem__("campaign_expected_runs", 46), "48-run A/A"),
            ("nonce", lambda x: x.__setitem__("nonce", "z" * 32), "nonce"),
            ("policy hash", lambda x: x.__setitem__("environment_policy_sha256", "f" * 64), "authenticated bytes"),
            ("quiescence", lambda x: x.__setitem__("guest_cpu_p95_percent", CPU_P95_MAX + .01), "exceed"),
            ("unknown", lambda x: x.__setitem__("unexpected", 1), "exact v2"),
        ):
            changed = copy.deepcopy(receipt); mutate(changed)
            try: validate_receipt(changed, result, {**hashes, **evidence_hashes}, environment)
            except ValueError as exc: assert message in str(exc), (name, exc)
            else: raise AssertionError(f"{name} mutation was accepted")
            tests += 1
        changed_result = copy.deepcopy(result); changed_result["measured_read_ops"] -= 1
        try: validate_artifacts(changed_result, hashes["result_sha256"], {**json.loads((root / "raw.json").read_text())}, hashes["raw_sha256"], json.loads((root / "done.json").read_text()), hashes["done_sha256"])
        except ValueError as exc: assert "count" in str(exc)
        else: raise AssertionError("geometry mutation was accepted")
        tests += 1
        changed_result = copy.deepcopy(result); changed_result["final_sha256"] = "c" * 64
        try: validate_artifacts(changed_result, hashes["result_sha256"], json.loads((root / "raw.json").read_text()), hashes["raw_sha256"], json.loads((root / "done.json").read_text()), hashes["done_sha256"])
        except ValueError as exc: assert "fixed last-write pattern" in str(exc)
        else: raise AssertionError("wrong deterministic pattern digest was accepted")
        tests += 1
        duplicate = root / "duplicate.json"
        duplicate.write_bytes(b'{"schema":"first","schema":"second"}')
        try: _read_compact_json(duplicate, "duplicate fixture")
        except ValueError as exc: assert "duplicate JSON key" in str(exc)
        else: raise AssertionError("duplicate workload JSON key was accepted")
        tests += 1
        thermal = json.loads((root / "thermal.json").read_text()); thermal["samples"][-1]["thermal_state"] = 1; (root / "thermal.json").write_bytes(_canonical(thermal) + b"\n")
        try: load_environment(root / "power.log", root / "environment.sh", root / "environment.py", root / "pmset.txt", root / "thermal.json",
                              root / "hid.json", root / "quiet.ps1", root / "quiet-config.json", root / "quiet.json", result["nonce"])
        except ValueError as exc: assert "nominal" in str(exc)
        else: raise AssertionError("non-nominal thermal evidence was accepted")
        tests += 1
        short_root = root / "short"; short_result, _, _, _ = _fixture(short_root)
        for name in ("thermal.json", "hid.json"):
            document = json.loads((short_root / name).read_text()); document["samples"].pop()
            (short_root / name).write_bytes(_canonical(document) + b"\n")
        try: load_environment(short_root / "power.log", short_root / "environment.sh", short_root / "environment.py",
            short_root / "pmset.txt", short_root / "thermal.json", short_root / "hid.json", short_root / "quiet.ps1",
            short_root / "quiet-config.json", short_root / "quiet.json", short_result["nonce"])
        except ValueError as exc: assert "at least 48" in str(exc)
        else: raise AssertionError("truncated host monitoring was accepted")
        tests += 1
        reset_root = root / "reset"; reset_result, _, _, _ = _fixture(reset_root)
        hid = json.loads((reset_root / "hid.json").read_text()); hid["samples"][1]["idle_nanoseconds"] = 1
        (reset_root / "hid.json").write_bytes(_canonical(hid) + b"\n")
        try: load_environment(reset_root / "power.log", reset_root / "environment.sh", reset_root / "environment.py",
            reset_root / "pmset.txt", reset_root / "thermal.json", reset_root / "hid.json", reset_root / "quiet.ps1",
            reset_root / "quiet-config.json", reset_root / "quiet.json", reset_result["nonce"])
        except ValueError as exc: assert "zero resets" in str(exc)
        else: raise AssertionError("host HID reset was accepted")
        tests += 1
        insecure_root = root / "insecure"; insecure_result, _, _, _ = _fixture(insecure_root)
        insecure = json.loads((insecure_root / "quiet.json").read_text()); insecure["security_services_enabled"] = False
        (insecure_root / "quiet.json").write_bytes(_canonical(insecure) + b"\n")
        try: load_environment(insecure_root / "power.log", insecure_root / "environment.sh", insecure_root / "environment.py",
            insecure_root / "pmset.txt", insecure_root / "thermal.json", insecure_root / "hid.json", insecure_root / "quiet.ps1",
            insecure_root / "quiet-config.json", insecure_root / "quiet.json", insecure_result["nonce"])
        except ValueError as exc: assert "identity" in str(exc)
        else: raise AssertionError("disabled guest security services were accepted")
        tests += 1
        _fixture(root / "fresh")
        quiet_path = root / "fresh" / "quiet.json"; quiet = json.loads(quiet_path.read_text()); quiet["samples"][28]["cpu_percent"] = 21.0; quiet["samples"][29]["cpu_percent"] = 21.0; quiet_path.write_bytes(_canonical(quiet) + b"\n")
        try: load_environment(root / "fresh" / "power.log", root / "fresh" / "environment.sh", root / "fresh" / "environment.py", root / "fresh" / "pmset.txt",
                              root / "fresh" / "thermal.json", root / "fresh" / "hid.json", root / "fresh" / "quiet.ps1",
                              root / "fresh" / "quiet-config.json", quiet_path, result["nonce"])
        except ValueError as exc: assert "ceiling" in str(exc)
        else: raise AssertionError("non-quiescent guest evidence was accepted")
        tests += 1
        v1 = copy.deepcopy(json.loads((root / "raw.json").read_text())); v1["schema"] = "bridgevm.windows-nvme-warm-seq.v1"
        try: _fixed_identity(v1, RAW_KEYS, "raw JSON")
        except ValueError as exc: assert "non-v2" in str(exc)
        else: raise AssertionError("v1 raw artifact was accepted")
        tests += 1
    print(f"HVF NVMe v2 receipt writer self-test: PASS ({tests} adversarial/positive checks)")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--result-json", type=Path); parser.add_argument("--raw-json", type=Path)
    parser.add_argument("--done-json", type=Path); parser.add_argument("--config-json", type=Path)
    parser.add_argument("--power-log", type=Path); parser.add_argument("--environment-policy", type=Path)
    parser.add_argument("--environment-helper", type=Path)
    parser.add_argument("--pmset-policy", type=Path); parser.add_argument("--thermal-log", type=Path)
    parser.add_argument("--hid-log", type=Path); parser.add_argument("--guest-quiescence-script", type=Path)
    parser.add_argument("--guest-quiescence-config", type=Path); parser.add_argument("--guest-quiescence-log", type=Path)
    parser.add_argument("--output", type=Path); parser.add_argument("--failed-reason")
    parser.add_argument("--validate", "--validate-receipt", dest="validate_receipt", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    try:
        if args.self_test: self_test(); return 0
        if args.validate_receipt:
            document, _ = _read_compact_json(args.validate_receipt, "receipt")
            if document.get("pass") is True:
                validation_inputs = (args.result_json, args.raw_json, args.done_json, args.config_json, args.power_log,
                    args.environment_policy, args.environment_helper, args.pmset_policy, args.thermal_log, args.hid_log,
                    args.guest_quiescence_script, args.guest_quiescence_config, args.guest_quiescence_log)
                if any(path is None for path in validation_inputs):
                    parser.error("passing receipt validation requires every private artifact and environment file")
                result, hashes = load_artifacts(args.result_json, args.raw_json, args.done_json, args.config_json)
                evidence_hashes, environment = load_environment(args.power_log, args.environment_policy,
                    args.environment_helper, args.pmset_policy, args.thermal_log, args.hid_log,
                    args.guest_quiescence_script, args.guest_quiescence_config, args.guest_quiescence_log, result["nonce"])
                validate_receipt(document, result, {**hashes, **evidence_hashes}, environment)
            else:
                validate_receipt(document)
            print("HVF NVMe v2 receipt: valid"); return 0
        if args.output is None: parser.error("--output is required")
        if args.output.exists() or args.output.is_symlink(): raise ValueError("receipt output must not already exist")
        actual = {"power_log_sha256": args.power_log, "environment_policy_sha256": args.environment_policy,
                  "environment_helper_sha256": args.environment_helper, "pmset_policy_sha256": args.pmset_policy,
                  "thermal_log_sha256": args.thermal_log, "hid_log_sha256": args.hid_log,
                  "guest_quiescence_script_sha256": args.guest_quiescence_script,
                  "guest_quiescence_config_sha256": args.guest_quiescence_config,
                  "guest_quiescence_log_sha256": args.guest_quiescence_log}
        if args.failed_reason:
            if any(path is not None for path in (args.result_json, args.raw_json, args.done_json, args.config_json)):
                parser.error("--failed-reason does not accept partial workload artifacts")
            receipt = build_failure(args.failed_reason, actual)
            with args.output.open("xb") as stream: stream.write(_canonical(receipt))
            return 0
        required = (args.result_json, args.raw_json, args.done_json, args.config_json, args.power_log,
                    args.environment_policy, args.environment_helper, args.pmset_policy, args.thermal_log, args.hid_log,
                    args.guest_quiescence_script, args.guest_quiescence_config, args.guest_quiescence_log, args.output)
        if any(path is None for path in required): parser.error("all evidence inputs and --output are required")
        result, hashes = load_artifacts(args.result_json, args.raw_json, args.done_json, args.config_json)
        evidence_hashes, environment = load_environment(args.power_log, args.environment_policy, args.environment_helper, args.pmset_policy,
            args.thermal_log, args.hid_log, args.guest_quiescence_script, args.guest_quiescence_config,
            args.guest_quiescence_log, result["nonce"])
        identity = _env_identity(result["nonce"], hashes, evidence_hashes, environment)
        receipt = build_receipt(identity, result, hashes, evidence_hashes, environment)
        with args.output.open("xb") as stream: stream.write(_canonical(receipt))
        return 0
    except (OSError, ValueError) as exc:
        print(f"HVF NVMe v2 receipt rejected: {exc}", file=sys.stderr); return 1


if __name__ == "__main__":
    raise SystemExit(main())
