#!/usr/bin/env python3
"""Fail-closed validation and paired statistics for sealed T16 NVMe campaigns."""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import math
import random
import re
import shutil
import statistics
import subprocess
import sys
import tempfile
from datetime import datetime, timedelta, timezone
from pathlib import Path
from typing import Any, Callable

TIER = "t16-hvf-nvme-performance"
GATE_ID = "hvf-nvme-performance-diagnostic"
WORKLOAD = "windows-nvme-warm-seq-v1"
CONFIG_SHA256 = "70da5472a5a7e89830ff240cfcb55dd3fff2b84e311016ac096d958879ec4c79"
KNOWN_CONFOUNDERS = [
    "guest-unbuffered sequential I/O uses a host-warm backing image",
    "storage and desktop timing share one end-to-end live attempt",
    "a host power-source event monitor runs concurrently",
]
RESOURCE_KEYS = {"image", "vars", "binary", "firmware", "renderer", "config", "workload_script", "campaign_registry"}
META_KEYS = {
    "campaign_id", "campaign_mode", "campaign_role", "campaign_ordinal",
    "campaign_expected_runs", "workload_profile", "file_mib", "transfer_kib",
    "read_passes", "write_passes",
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
COMMON_FIELDS = (
    "harness_commit", "image_sha256", "vars_sha256", "firmware_sha256",
    "renderer_sha256", "config_sha256", "host_model",
    "macos_version", "power_source_start", "power_source_end", "workload_profile", "file_mib",
    "transfer_kib", "read_passes", "write_passes", "smp_cpus", "ram_mib",
    "known_confounders", "warmup_sha256", "final_sha256",
)
METRICS = {
    "read_throughput_mib_s": {"direction": "higher", "role": "primary"},
    "read_p99_ms": {"direction": "lower", "role": "read-tail-non-regression"},
    "write_durable_throughput_mib_s": {"direction": "higher", "role": "non-regression"},
    "write_p99_ms": {"direction": "lower", "role": "non-regression"},
    "flush_max_ms": {"direction": "lower", "role": "non-regression"},
    "desktop_elapsed_ms": {"direction": "lower", "role": "non-regression"},
}
AA_FIXTURE_ID, AB_FIXTURE_ID = "0" * 32, "1" * 32
ROOT = Path(__file__).resolve().parent.parent
ANALYZER_FILES = (
    "scripts/hvf_nvme_performance_report.py",
    "scripts/write-hvf-nvme-performance-receipt.py",
    "scripts/live-gates/redact-receipt.py",
)


class EvidenceError(Exception):
    def __init__(self, errors: list[str], attempts: list[dict[str, Any]] | None = None):
        super().__init__("; ".join(errors)); self.errors, self.attempts = errors, attempts or []

    def document(self) -> dict[str, Any]:
        return {"error": "campaign evidence is incomplete or invalid", "errors": self.errors,
                "attempts": self.attempts, "claim_eligible": False}


def _load_local_module(name: str, path: Path) -> Any:
    specification = importlib.util.spec_from_file_location(name, path)
    if specification is None or specification.loader is None:
        raise RuntimeError(f"cannot load evidence validator {path}")
    module = importlib.util.module_from_spec(specification); specification.loader.exec_module(module)
    return module


WRITER = _load_local_module("bridgevm_t16_writer", ROOT / "scripts/write-hvf-nvme-performance-receipt.py")
REDACTOR = _load_local_module("bridgevm_receipt_redactor", ROOT / "scripts/live-gates/redact-receipt.py")


def _seal_analyzer(commit: str) -> dict[str, Any]:
    head = subprocess.check_output(["git", "-C", str(ROOT), "rev-parse", "HEAD"], text=True).strip()
    if head != commit:
        raise ValueError("reporter checkout does not match the campaign harness commit")
    dirty = subprocess.check_output(["git", "-C", str(ROOT), "status", "--porcelain", "--untracked-files=all", "--", *ANALYZER_FILES], text=True)
    if dirty:
        raise ValueError("reporter or evidence validators have uncommitted bytes")
    hashes: dict[str, str] = {}
    for relative in ANALYZER_FILES:
        path = ROOT / relative; committed = subprocess.check_output(["git", "-C", str(ROOT), "show", f"{commit}:{relative}"])
        current = path.read_bytes()
        if current != committed:
            raise ValueError(f"executing analyzer dependency differs from {commit}:{relative}")
        hashes[relative] = hashlib.sha256(current).hexdigest()
    return {"harness_commit": commit, "source_sha256": hashes}


def _hex(value: Any, width: int) -> bool:
    return isinstance(value, str) and bool(re.fullmatch(f"[0-9a-f]{{{width}}}", value))


def _integer(value: str, name: str) -> int:
    if not value.isascii() or not value.isdigit() or str(int(value)) != value or int(value) < 1:
        raise ValueError(f"{name} must be a canonical positive integer")
    return int(value)


def _number(value: Any, name: str) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(float(value)) or value <= 0:
        raise ValueError(f"{name} must be finite and positive")
    return float(value)


def _timestamp(value: Any, name: str) -> datetime:
    if not isinstance(value, str):
        raise ValueError(f"{name} must be an ISO-8601 timestamp")
    try:
        parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError as exc:
        raise ValueError(f"{name} must be an ISO-8601 timestamp") from exc
    if parsed.tzinfo is None:
        raise ValueError(f"{name} must include a timezone")
    return parsed


def _rows(path: Path) -> list[list[str]]:
    return [line.split("\t") for line in path.read_text(encoding="utf-8").splitlines() if line]


def _mentions(path: Path, campaign_id: str) -> bool:
    try:
        return any(len(row) >= 2 and row[0] == "campaign_id" and row[1] == campaign_id for row in _rows(path))
    except (OSError, UnicodeError):
        return False


def _expected_role(ordinal: int) -> str:
    return "baseline" if (((ordinal - 1) // 2 + ordinal) % 2 == 1) else "candidate"


def _manifest(path: Path) -> tuple[dict[str, str], dict[str, tuple[str, str]]]:
    metadata: dict[str, str] = {}; resources: dict[str, tuple[str, str]] = {}; rows = _rows(path)
    for row in rows:
        key = row[0] if row else ""
        if key in metadata or key in resources:
            raise ValueError(f"duplicate manifest key {key!r}")
        if key in RESOURCE_KEYS and len(row) == 3:
            if not Path(row[1]).is_absolute() or not _hex(row[2], 64):
                raise ValueError(f"invalid resource row {key!r}")
            resources[key] = (row[1], row[2])
        elif key in META_KEYS and len(row) == 2 and row[1]:
            metadata[key] = row[1]
        else:
            raise ValueError(f"invalid manifest row for {key!r}")
    if len(rows) != 18 or set(resources) != RESOURCE_KEYS or set(metadata) != META_KEYS:
        raise ValueError("manifest must contain exactly 18 unique resource/metadata rows")
    fixed = {"workload_profile": WORKLOAD, "file_mib": "512",
             "transfer_kib": "128", "read_passes": "5", "write_passes": "2"}
    if any(metadata[field] != expected for field, expected in fixed.items()):
        raise ValueError("manifest build or workload constants are invalid")
    if not _hex(metadata["campaign_id"], 32):
        raise ValueError("manifest campaign id is not canonical")
    ordinal = _integer(metadata["campaign_ordinal"], "campaign_ordinal")
    expected = _integer(metadata["campaign_expected_runs"], "campaign_expected_runs")
    if expected < 20 or expected > 200 or expected % 4 or ordinal > expected:
        raise ValueError("campaign expected runs must be a multiple of four in 20..200")
    if metadata["campaign_mode"] not in {"AA", "AB"} or metadata["campaign_role"] != _expected_role(ordinal):
        raise ValueError("manifest mode or counterbalanced role is invalid")
    if resources["config"][1] != CONFIG_SHA256:
        raise ValueError("manifest config hash is not the fixed workload config")
    return metadata, resources


def _env(path: Path) -> dict[str, str]:
    result: dict[str, str] = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        key, separator, value = line.partition("=")
        if not separator or not key or key in result:
            raise ValueError(f"malformed or duplicate {path.name} entry")
        result[key] = value
    return result


def _authenticated_receipt(directory: Path) -> dict[str, Any]:
    private_path, public_path, share = directory / "receipt.json", directory / "receipt.public.json", directory / "share"
    private = json.loads(private_path.read_text(encoding="utf-8"))
    result, hashes = WRITER._load_artifacts(share / "nvme-result.json", share / "nvme-raw.json", share / "nvme-result.done", share / "nvme-workload-config.json")
    WRITER.validate_receipt(private, result, hashes)
    public = json.loads(public_path.read_text(encoding="utf-8"))
    if public != REDACTOR.redact(private):
        raise ValueError("public receipt is not the exact redaction of its validated private receipt")
    power_path = directory / "power-source.log"
    if not power_path.is_file() or power_path.is_symlink() or hashlib.sha256(power_path.read_bytes()).hexdigest() != public["power_log_sha256"]:
        raise ValueError("power event log is missing, unsafe, or differs from its receipt hash")
    sources = [line for line in power_path.read_text(encoding="utf-8").splitlines() if line.startswith("Now drawing from ")]
    if not sources or any(line != "Now drawing from 'AC Power'" for line in sources):
        raise ValueError("power event log does not prove an uninterrupted AC-Power observation")
    return public


def _receipt_fixed(receipt: dict[str, Any]) -> None:
    if set(receipt) != RECEIPT_KEYS:
        raise ValueError("receipt has missing or unknown fields")
    fixed = {
        "schema_version": 1, "tier": TIER, "gate_id": GATE_ID,
        "workload_profile": WORKLOAD, "file_mib": 512, "transfer_kib": 128,
        "read_passes": 5, "write_passes": 2, "file_bytes": 536_870_912,
        "transfer_bytes": 131_072, "blocks_per_pass": 4_096,
        "precondition_write_ops": 4_096, "precondition_write_bytes": 536_870_912,
        "warmup_read_ops": 4_096, "warmup_read_bytes": 536_870_912,
        "measured_read_ops": 20_480, "measured_read_bytes": 2_684_354_560,
        "measured_write_ops": 8_192, "measured_write_bytes": 1_073_741_824,
        "write_verify_read_ops": 8_192,
        "read_result_count": 5, "write_result_count": 2,
        "final_verify_read_ops": 4_096, "final_verify_read_bytes": 536_870_912,
        "verified_read_ops": 32_768, "flush_calls": 3, "sample_count": 1,
        "run_count": 1, "required_run_count": 1, "passes": 1, "failures": 0,
        "outcome": "completed", "pass": True, "valid": True,
        "invalid_reason": "", "evidence_paths": ["share/nvme-result.json", "power-source.log"],
        "smp_cpus": 4, "ram_mib": 6144, "known_confounders": KNOWN_CONFOUNDERS,
    }
    for field, expected in fixed.items():
        actual = receipt.get(field)
        if actual != expected or (type(expected) is int and type(actual) is not int):
            raise ValueError(f"receipt has invalid {field}")
    for field in HASH_FIELDS:
        if not _hex(receipt.get(field), 64):
            raise ValueError(f"receipt {field} must be 64 lowercase hex characters")
    if receipt["config_sha256"] != CONFIG_SHA256:
        raise ValueError("receipt config hash is not the fixed workload config")
    for field in PERFORMANCE_FIELDS + ("desktop_elapsed_ms",):
        _number(receipt.get(field), field)
    for field in ("bytes_per_sector", "file_alignment_bytes", "required_alignment_bytes", "read_phase_elapsed_ns", "read_service_elapsed_ns", "write_phase_elapsed_ns", "write_and_flush_service_elapsed_ns"):
        if type(receipt.get(field)) is not int or receipt[field] <= 0:
            raise ValueError(f"receipt {field} must be a positive integer")
    if receipt["required_alignment_bytes"] != max(receipt["bytes_per_sector"], receipt["file_alignment_bytes"]) or receipt["transfer_bytes"] % receipt["required_alignment_bytes"]:
        raise ValueError("receipt storage alignment is inconsistent")
    if receipt["read_phase_elapsed_ns"] < receipt["read_service_elapsed_ns"] or receipt["write_phase_elapsed_ns"] < receipt["write_and_flush_service_elapsed_ns"]:
        raise ValueError("receipt phase elapsed time is shorter than service time")
    rate_checks = {
        "read_phase_mib_per_sec": 2560.0 / (receipt["read_phase_elapsed_ns"] / 1e9),
        "read_service_mib_per_sec": 2560.0 / (receipt["read_service_elapsed_ns"] / 1e9),
        "write_durable_mib_per_sec": 1024.0 / (receipt["write_phase_elapsed_ns"] / 1e9),
        "write_and_flush_service_mib_per_sec": 1024.0 / (receipt["write_and_flush_service_elapsed_ns"] / 1e9),
    }
    for field, expected in rate_checks.items():
        if not math.isclose(float(receipt[field]), expected, rel_tol=1e-12, abs_tol=1e-12):
            raise ValueError(f"receipt {field} is inconsistent with bytes and elapsed time")
    if receipt["read_throughput_mib_s"] != receipt["read_phase_mib_per_sec"] or receipt["write_durable_throughput_mib_s"] != receipt["write_durable_mib_per_sec"]:
        raise ValueError("receipt canonical throughput aliases disagree")
    for prefix in ("read", "write", "flush"):
        values = [receipt[f"{prefix}_{name}_ms"] for name in ("p50", "p95", "p99", "max")]
        if values != sorted(values):
            raise ValueError(f"receipt {prefix} latency percentiles are not monotonic")


def _validate_receipt(job: dict[str, Any], metadata: dict[str, str], resources: dict[str, tuple[str, str]]) -> None:
    receipt = job["receipt"]
    expected: dict[str, Any] = {
        "campaign_id": metadata["campaign_id"], "campaign_mode": metadata["campaign_mode"],
        "campaign_role": metadata["campaign_role"], "campaign_ordinal": int(metadata["campaign_ordinal"]),
        "campaign_expected_runs": int(metadata["campaign_expected_runs"]),
        "image_sha256": resources["image"][1], "vars_sha256": resources["vars"][1],
        "binary_hash": resources["binary"][1], "firmware_sha256": resources["firmware"][1],
        "renderer_sha256": resources["renderer"][1], "config_sha256": resources["config"][1],
        "workload_script_sha256": resources["workload_script"][1],
        "campaign_registry_sha256": resources["campaign_registry"][1],
    }
    for field, value in expected.items():
        actual = receipt.get(field)
        if actual != value or (type(value) is int and type(actual) is not int):
            raise ValueError(f"receipt {field} does not match its sealed manifest")
    _receipt_fixed(receipt)
    if receipt.get("job_id") != job["job_id"]:
        raise ValueError("receipt job_id does not match queue directory")
    if job["job_env"].get("job_id") != job["job_id"] or job["job_env"].get("tier") != TIER:
        raise ValueError("job.env identity does not match queue directory and tier")
    if job["result_env"].get("result") != "pass" or job["result_env"].get("exit_code") != "0":
        raise ValueError("worker result is not a successful zero-exit attempt")
    harness = receipt.get("harness_commit")
    if not _hex(harness, 40) or harness != job["job_env"].get("commit") or receipt.get("commit") != harness:
        raise ValueError("receipt harness commit does not match the sealed queue commit")
    manifest_hash = hashlib.sha256(job["manifest_path"].read_bytes()).hexdigest()
    if receipt.get("input_manifest_sha256") != manifest_hash or job["job_env"].get("input_manifest_sha256") != manifest_hash:
        raise ValueError("receipt and queue do not seal the input manifest")
    if job["job_env"].get("sealed_binary_sha256") != resources["binary"][1]:
        raise ValueError("queue binary identity does not match the manifest")
    ledger = _env(job["manifest_path"].parents[2] / "job-ledger" / job["job_id"] / "entry.env")
    ledger_keys = {"job_id", "tier", "commit", "input_manifest_sha256", "sealed_binary_sha256"}
    if set(ledger) != ledger_keys or any(ledger[key] != job["job_env"].get(key) for key in ledger_keys):
        raise ValueError("durable submission ledger does not exactly match job.env")
    expected_nonce = hashlib.sha256(f"{metadata['campaign_id']}:{metadata['campaign_ordinal']}".encode()).hexdigest()[:32]
    if receipt.get("nonce") != expected_nonce:
        raise ValueError("receipt nonce does not match the sealed campaign lane")
    for field in ("host_model", "macos_version"):
        if not isinstance(receipt.get(field), str) or receipt[field].strip().lower() in {"", "unknown"}:
            raise ValueError(f"receipt has missing or unknown {field}")
    if any(receipt.get(field) != "AC Power" for field in ("power_source", "power_source_start", "power_source_end")):
        raise ValueError("receipt was not measured entirely on AC Power")
    started, finished = _timestamp(receipt.get("started_at"), "started_at"), _timestamp(receipt.get("finished_at"), "finished_at")
    if finished <= started:
        raise ValueError("finished_at must be after started_at")
    job["started"], job["finished"] = started, finished


def _attempt(job: dict[str, Any]) -> dict[str, Any]:
    metadata, receipt = job.get("metadata", {}), job.get("receipt", {})
    return {"job_id": job["job_id"], "state": job["state"], "campaign_id": metadata.get("campaign_id"),
            "mode": metadata.get("campaign_mode"), "role": metadata.get("campaign_role"),
            "ordinal": int(metadata["campaign_ordinal"]) if metadata.get("campaign_ordinal", "").isdigit() else None,
            "started_at": receipt.get("started_at"), "finished_at": receipt.get("finished_at"),
            "pass": receipt.get("pass"), "valid": receipt.get("valid"), "invalid_reason": receipt.get("invalid_reason")}


def _campaign_registry(root: Path, campaign_id: str, mode: str) -> dict[str, Any]:
    path = root / "campaign-inputs" / campaign_id / "campaign-registry.tsv"
    if not path.is_file() or path.is_symlink():
        raise ValueError("canonical campaign registry is missing or unsafe")
    rows = _rows(path)
    if len(rows) < 26 or rows[:3] != [["schema", "bridgevm.t16-campaign-registry.v1"], ["campaign_id", campaign_id], ["campaign_mode", mode]]:
        raise ValueError("campaign registry header is invalid")
    header = {row[0]: row[1] for row in rows[:6] if len(row) == 2}
    if set(header) != {"schema", "campaign_id", "campaign_mode", "pairs", "expected_runs", "harness_commit"}:
        raise ValueError("campaign registry metadata is incomplete or duplicated")
    pairs = _integer(header["pairs"], "registry pairs"); expected = _integer(header["expected_runs"], "registry expected_runs")
    if pairs < 10 or pairs > 100 or pairs % 2 or expected != pairs * 2 or not _hex(header["harness_commit"], 40):
        raise ValueError("campaign registry shape or harness commit is invalid")
    lanes: dict[int, tuple[str, str]] = {}
    for row in rows[6:]:
        if len(row) != 4 or row[0] != "lane": raise ValueError("campaign registry lane row is invalid")
        ordinal = _integer(row[1], "registry ordinal"); expected_job = f"t16-{campaign_id}-{ordinal:03d}"
        if ordinal in lanes or ordinal > expected or row[2] != _expected_role(ordinal) or row[3] != expected_job:
            raise ValueError("campaign registry lane identity is invalid")
        lanes[ordinal] = (row[2], row[3])
    if sorted(lanes) != list(range(1, expected + 1)) or len(rows) != 6 + expected:
        raise ValueError("campaign registry does not exactly cover every lane")
    return {"path": path, "sha256": hashlib.sha256(path.read_bytes()).hexdigest(), "harness_commit": header["harness_commit"], "expected": expected, "lanes": lanes}


def _discover(root: Path, campaign_id: str) -> list[dict[str, Any]]:
    jobs: list[dict[str, Any]] = []
    for state in ("queued", "running", "done"):
        directory = root / state
        if directory.is_dir():
            for manifest in sorted(directory.glob("*/input-manifest.tsv")):
                if _mentions(manifest, campaign_id):
                    jobs.append({"job_id": manifest.parent.name, "state": state, "manifest_path": manifest})
    return jobs


def load_campaign(root: Path, campaign_id: str, mode: str) -> dict[str, Any]:
    try: registry = _campaign_registry(root, campaign_id, mode)
    except (OSError, UnicodeError, ValueError) as exc: raise EvidenceError([f"campaign {campaign_id!r}: {exc}"]) from exc
    jobs = _discover(root, campaign_id); errors: list[str] = []
    if not jobs: errors.append(f"campaign {campaign_id!r} has no registered attempts")
    for job in jobs:
        try:
            metadata, resources = _manifest(job["manifest_path"]); job["metadata"], job["resources"] = metadata, resources
            if metadata["campaign_id"] != campaign_id or metadata["campaign_mode"] != mode:
                raise ValueError(f"campaign must have id {campaign_id!r} and mode {mode}")
            ordinal = int(metadata["campaign_ordinal"])
            if (Path(resources["campaign_registry"][0]).resolve(strict=True), resources["campaign_registry"][1]) != (registry["path"], registry["sha256"]):
                raise ValueError("manifest does not seal the canonical campaign registry")
            if metadata["campaign_expected_runs"] != str(registry["expected"]) or registry["lanes"].get(ordinal) != (metadata["campaign_role"], job["job_id"]):
                raise ValueError("job does not match its immutable registry lane")
            if job["state"] != "done":
                raise ValueError(f"attempt remains in {job['state']} state")
            job["job_env"] = _env(job["manifest_path"].parent / "job.env")
            job["result_env"] = _env(job["manifest_path"].parent / "result.env")
            job["receipt"] = _authenticated_receipt(job["manifest_path"].parent)
            _validate_receipt(job, metadata, resources)
        except (OSError, UnicodeError, json.JSONDecodeError, ValueError) as exc:
            errors.append(f"{job['job_id']}: {exc}")
    attempts = [_attempt(job) for job in jobs]; valid = [job for job in jobs if "started" in job]
    ids = [job["job_id"] for job in jobs]
    if len(ids) != len(set(ids)): errors.append("campaign contains duplicate job_ids")
    if set(ids) != {job_id for _, job_id in registry["lanes"].values()}:
        errors.append("registered job IDs do not exactly match discovered attempts")
    ordered = sorted(valid, key=lambda job: int(job["metadata"]["campaign_ordinal"]))
    if ordered:
        expected_values = {int(job["metadata"]["campaign_expected_runs"]) for job in ordered}
        if len(expected_values) != 1:
            errors.append("campaign_expected_runs is inconsistent")
        else:
            expected = next(iter(expected_values)); ordinals = [int(job["metadata"]["campaign_ordinal"]) for job in ordered]
            if ordinals != list(range(1, expected + 1)):
                errors.append(f"campaign ordinals do not exactly cover 1..{expected}")
        for index, job in enumerate(ordered):
            ordinal = int(job["metadata"]["campaign_ordinal"])
            if job["metadata"]["campaign_role"] != _expected_role(ordinal):
                errors.append(f"ordinal {ordinal} violates the counterbalanced role schedule")
            if index and job["started"] < ordered[index - 1]["finished"]:
                errors.append(f"ordinal {index + 1} overlaps its predecessor")
    if errors: raise EvidenceError(errors, attempts)
    reference = ordered[0]["receipt"]
    if reference["harness_commit"] != registry["harness_commit"]:
        errors.append("campaign registry harness commit does not match its receipts")
    for job in ordered[1:]:
        for field in COMMON_FIELDS:
            if job["receipt"].get(field) != reference.get(field):
                errors.append(f"campaign has mismatched common identity {field}")
    nonces = [job["receipt"]["nonce"] for job in ordered]
    if len(nonces) != len(set(nonces)): errors.append("campaign reuses a workload nonce")
    scripts = [job["receipt"]["workload_script_sha256"] for job in ordered]
    if len(scripts) != len(set(scripts)): errors.append("campaign reuses a workload script")
    baseline = [job for job in ordered if job["metadata"]["campaign_role"] == "baseline"]
    candidate = [job for job in ordered if job["metadata"]["campaign_role"] == "candidate"]
    for side, name in ((baseline, "baseline"), (candidate, "candidate")):
        if len({job["receipt"]["binary_hash"] for job in side}) != 1:
            errors.append(f"{name} does not use one sealed binary identity")
    left, right = baseline[0]["receipt"], candidate[0]["receipt"]
    same = left["binary_hash"] == right["binary_hash"]
    if (mode == "AA" and not same) or (mode == "AB" and same): errors.append(f"{mode} campaign has invalid binary relationship")
    if mode == "AB" and left["binary_hash"] == right["binary_hash"]:
        errors.append("AB requires distinct binary hashes")
    if errors: raise EvidenceError(errors, attempts)
    return {"id": campaign_id, "mode": mode, "jobs": ordered, "attempts": attempts}


def _paired(campaign: dict[str, Any], metric: str) -> tuple[list[float], list[float], list[float]]:
    before: list[float] = []; after: list[float] = []
    jobs = campaign["jobs"]
    for index in range(0, len(jobs), 2):
        pair = jobs[index:index + 2]
        baseline = next(job for job in pair if job["metadata"]["campaign_role"] == "baseline")
        candidate = next(job for job in pair if job["metadata"]["campaign_role"] == "candidate")
        before.append(float(baseline["receipt"][metric])); after.append(float(candidate["receipt"][metric]))
    higher = METRICS[metric]["direction"] == "higher"
    deltas = [100.0 * ((candidate - baseline) if higher else (baseline - candidate)) / baseline for baseline, candidate in zip(before, after)]
    return before, after, deltas


def _interval(deltas: list[float], metric: str, mode: str) -> tuple[float, float, float]:
    seed = int.from_bytes(hashlib.sha256(f"t16:{mode}:{metric}".encode()).digest()[:8], "big")
    rng = random.Random(seed)
    boot = sorted(statistics.median(rng.choices(deltas, k=len(deltas))) for _ in range(10_000))
    return statistics.median(deltas), boot[249], boot[9749]


def _summary(campaign: dict[str, Any]) -> dict[str, Any]:
    metrics: dict[str, Any] = {}
    for metric, definition in METRICS.items():
        before, after, deltas = _paired(campaign, metric); estimate, lower, upper = _interval(deltas, metric, campaign["mode"])
        metrics[metric] = {"direction": definition["direction"], "role": definition["role"],
                           "baseline_samples": before, "candidate_samples": after,
                           "paired_directional_delta_percent": estimate,
                           "paired_directional_delta_percent_ci95": [lower, upper],
                           "bootstrap_samples": 10_000}
    reference = campaign["jobs"][0]["receipt"]
    return {"campaign_id": campaign["id"], "mode": campaign["mode"],
            "expected_runs": len(campaign["jobs"]), "pairs": len(campaign["jobs"]) // 2,
            "identity": {field: reference[field] for field in COMMON_FIELDS},
            "attempts": campaign["attempts"], "metrics": metrics}


def analyze(root: Path, aa_id: str, ab_id: str | None = None, *, seal_analyzer: bool = True) -> dict[str, Any]:
    root = root.resolve(strict=True)
    aa = load_campaign(root, aa_id, "AA"); aa_summary = _summary(aa)
    try: analysis_identity = _seal_analyzer(aa["jobs"][0]["receipt"]["harness_commit"]) if seal_analyzer else None
    except (OSError, subprocess.SubprocessError, ValueError) as exc: raise EvidenceError([f"analyzer is not sealed: {exc}"], aa["attempts"]) from exc
    bounds = {metric: max(abs(data["paired_directional_delta_percent_ci95"][0]), abs(data["paired_directional_delta_percent_ci95"][1])) for metric, data in aa_summary["metrics"].items()}
    result: dict[str, Any] = {"claim_eligible": False, "claim_rule": "read throughput CI must exceed its A/A noise bound; read p99 and every named guardrail CI must exclude regression beyond its own A/A noise bound",
                              "aa": aa_summary, "aa_noise_bounds_percent": bounds}
    if analysis_identity is not None: result["analysis_identity"] = analysis_identity
    if ab_id is None: return result
    ab = load_campaign(root, ab_id, "AB")
    combined = aa["jobs"] + ab["jobs"]
    errors: list[str] = []
    if len({job["job_id"] for job in combined}) != len(combined): errors.append("AA and AB campaigns reuse a job_id")
    if len({job["receipt"]["nonce"] for job in combined}) != len(combined): errors.append("AA and AB campaigns reuse a workload nonce")
    if len({job["receipt"]["workload_script_sha256"] for job in combined}) != len(combined): errors.append("AA and AB campaigns reuse a workload script")
    if ab["jobs"][0]["started"] < aa["jobs"][-1]["finished"]: errors.append("AB campaign overlaps the A/A campaign")
    aa_ref, ab_ref = aa["jobs"][0]["receipt"], ab["jobs"][0]["receipt"]
    for field in COMMON_FIELDS:
        if aa_ref[field] != ab_ref[field]: errors.append(f"AA and AB have mismatched common identity {field}")
    if aa_ref["campaign_expected_runs"] != ab_ref["campaign_expected_runs"]: errors.append("AA and AB expected run counts differ")
    ab_baseline = next(job for job in ab["jobs"] if job["metadata"]["campaign_role"] == "baseline")["receipt"]
    if aa_ref["binary_hash"] != ab_baseline["binary_hash"]:
        errors.append("AB baseline does not match the AA binary identity")
    if errors: raise EvidenceError(errors, aa["attempts"] + ab["attempts"])
    ab_summary = _summary(ab); result["ab"] = ab_summary
    decisions: dict[str, Any] = {}
    for metric, definition in METRICS.items():
        lower = ab_summary["metrics"][metric]["paired_directional_delta_percent_ci95"][0]
        threshold = bounds[metric] if definition["role"] == "primary" else -bounds[metric]
        passed = lower > threshold if definition["role"] == "primary" else lower >= threshold
        decisions[metric] = {"role": definition["role"], "ci95_lower_percent": lower,
                             "required_lower_bound_percent": threshold, "pass": passed}
    result["primary_read_throughput"] = decisions.pop("read_throughput_mib_s")
    result["read_tail_non_regression"] = decisions.pop("read_p99_ms")
    result["non_regressions"] = decisions
    result["claim_eligible"] = result["primary_read_throughput"]["pass"] and result["read_tail_non_regression"]["pass"] and all(item["pass"] for item in decisions.values())
    return result


def _private_fixture(directory: Path, receipt: dict[str, Any], nonce: str, candidate: bool) -> None:
    power = directory / "power-source.log"; power.write_text("Now drawing from 'AC Power'\n")
    receipt["power_log_sha256"] = hashlib.sha256(power.read_bytes()).hexdigest()
    share = directory / "share"; share.mkdir()
    result_path, raw_path, done_path, config_path = WRITER._fixture(share)
    raw = json.loads(raw_path.read_text()); raw["nonce"] = nonce
    raw_path.write_text(json.dumps(raw, separators=(",", ":"))); raw_hash = hashlib.sha256(raw_path.read_bytes()).hexdigest()
    result = json.loads(result_path.read_text()); result["nonce"] = nonce; result["raw_sha256"] = raw_hash
    if candidate:
        result["read_phase_elapsed_ns"] = result["read_service_elapsed_ns"] + 500_000
        result["read_phase_mib_per_sec"] = 2560.0 / (result["read_phase_elapsed_ns"] / 1e9)
        result["read_throughput_mib_s"] = result["read_phase_mib_per_sec"]
    result_path.write_text(json.dumps(result, separators=(",", ":"))); result_hash = hashlib.sha256(result_path.read_bytes()).hexdigest()
    done = json.loads(done_path.read_text()); done.update({"nonce": nonce, "raw_sha256": raw_hash, "result_sha256": result_hash})
    done_path.write_text(json.dumps(done, separators=(",", ":")))
    result_path.rename(share / "nvme-result.json"); raw_path.rename(share / "nvme-raw.json")
    done_path.rename(share / "nvme-result.done"); config_path.rename(share / "nvme-workload-config.json")
    result, hashes = WRITER._load_artifacts(share / "nvme-result.json", share / "nvme-raw.json", share / "nvme-result.done", share / "nvme-workload-config.json")
    receipt.update(hashes)
    for field in COUNT_FIELDS + PERFORMANCE_FIELDS + ("warmup_sha256", "final_sha256", "nonce"):
        receipt[field] = result[field]
    WRITER.validate_receipt(receipt, result, hashes)
    (directory / "receipt.json").write_text(json.dumps(receipt))
    (directory / "receipt.public.json").write_text(json.dumps(REDACTOR.redact(receipt)))


def _fixture(root: Path, guardrail_regression: bool = False) -> None:
    def campaign(identifier: str, mode: str, day: int) -> None:
        registry_path = root / "campaign-inputs" / identifier / "campaign-registry.tsv"; registry_path.parent.mkdir(parents=True)
        registry_rows = [["schema", "bridgevm.t16-campaign-registry.v1"], ["campaign_id", identifier], ["campaign_mode", mode], ["pairs", "10"], ["expected_runs", "20"], ["harness_commit", "1" * 40]]
        registry_rows.extend([["lane", str(ordinal), _expected_role(ordinal), f"t16-{identifier}-{ordinal:03d}"] for ordinal in range(1, 21)])
        registry_path.write_text("\n".join("\t".join(row) for row in registry_rows) + "\n"); registry_hash = hashlib.sha256(registry_path.read_bytes()).hexdigest()
        for offset in range(20):
            ordinal, role = offset + 1, _expected_role(offset + 1); job_id = f"t16-{identifier}-{ordinal:03d}"
            directory = root / "done" / job_id; directory.mkdir(parents=True)
            candidate = mode == "AB" and role == "candidate"
            binary = "e" * 64 if candidate else "d" * 64
            script_hash = hashlib.sha256(f"script-{job_id}".encode()).hexdigest()
            metadata = {"campaign_id": identifier, "campaign_mode": mode,
                        "campaign_role": role, "campaign_ordinal": str(ordinal), "campaign_expected_runs": "20",
                        "workload_profile": WORKLOAD, "file_mib": "512", "transfer_kib": "128", "read_passes": "5", "write_passes": "2"}
            resource_hashes = {"image": "a" * 64, "vars": "b" * 64, "binary": binary,
                               "firmware": "f" * 64, "renderer": "9" * 64,
                               "config": CONFIG_SHA256, "workload_script": script_hash,
                               "campaign_registry": registry_hash}
            rows = [f"{key}\t{registry_path if key == 'campaign_registry' else f'/sealed/{key}'}\t{value}" for key, value in resource_hashes.items()] + [f"{key}\t{value}" for key, value in metadata.items()]
            manifest = directory / "input-manifest.tsv"; manifest.write_text("\n".join(rows) + "\n")
            manifest_hash = hashlib.sha256(manifest.read_bytes()).hexdigest()
            (directory / "job.env").write_text(f"job_id={job_id}\ntier={TIER}\ncommit={'1' * 40}\ninput_manifest_sha256={manifest_hash}\nsealed_binary_sha256={binary}\n")
            (directory / "result.env").write_text("result=pass\nexit_code=0\n")
            ledger = root / "job-ledger" / job_id; ledger.mkdir(parents=True)
            (ledger / "entry.env").write_text(f"job_id={job_id}\ntier={TIER}\ncommit={'1' * 40}\ninput_manifest_sha256={manifest_hash}\nsealed_binary_sha256={binary}\n")
            started = datetime(2026, 1, day, tzinfo=timezone.utc) + timedelta(minutes=offset * 2); finished = started + timedelta(minutes=1)
            read_p99 = 4.75 if candidate else 5.0
            read_phase = 23_272_727_273 if candidate else 25_600_000_000
            read_rate = 2560.0 / (read_phase / 1e9)
            read_service, write_phase, write_service = 20_480_000_000, 12_800_000_000, 10_240_000_000
            receipt: dict[str, Any] = {"schema_version": 1, "tier": TIER, "gate_id": GATE_ID,
                "job_id": job_id, "commit": "1" * 40, "harness_commit": "1" * 40,
                "binary_hash": binary,
                "input_manifest_sha256": manifest_hash, "image_sha256": "a" * 64, "vars_sha256": "b" * 64,
                "firmware_sha256": "f" * 64, "renderer_sha256": "9" * 64, "config_sha256": CONFIG_SHA256,
                "workload_script_sha256": script_hash, "campaign_registry_sha256": registry_hash,
                "raw_sha256": hashlib.sha256(f"raw-{job_id}".encode()).hexdigest(),
                "result_sha256": hashlib.sha256(f"result-{job_id}".encode()).hexdigest(), "done_sha256": hashlib.sha256(f"done-{job_id}".encode()).hexdigest(),
                "warmup_sha256": "6" * 64, "final_sha256": "7" * 64, "workload_profile": WORKLOAD,
                "nonce": hashlib.sha256(f"{identifier}:{ordinal}".encode()).hexdigest()[:32], "campaign_id": identifier, "campaign_mode": mode,
                "campaign_role": role, "campaign_ordinal": ordinal, "campaign_expected_runs": 20,
                "host_model": "MacTest", "macos_version": "26.0", "power_source": "AC Power",
                "power_source_start": "AC Power", "power_source_end": "AC Power", "file_mib": 512,
                "transfer_kib": 128, "read_passes": 5, "write_passes": 2,
                "smp_cpus": 4, "ram_mib": 6144, "known_confounders": KNOWN_CONFOUNDERS,
                "file_bytes": 536_870_912, "transfer_bytes": 131_072, "blocks_per_pass": 4_096,
                "precondition_write_ops": 4_096, "precondition_write_bytes": 536_870_912,
                "warmup_read_ops": 4_096, "warmup_read_bytes": 536_870_912,
                "measured_read_ops": 20_480, "measured_read_bytes": 2_684_354_560,
                "measured_write_ops": 8_192, "measured_write_bytes": 1_073_741_824,
                "write_verify_read_ops": 8_192,
                "read_result_count": 5, "write_result_count": 2, "final_verify_read_ops": 4_096,
                "final_verify_read_bytes": 536_870_912, "verified_read_ops": 32_768, "flush_calls": 3,
                "bytes_per_sector": 512, "file_alignment_bytes": 4096, "required_alignment_bytes": 4096,
                "read_phase_elapsed_ns": read_phase, "read_service_elapsed_ns": read_service,
                "write_phase_elapsed_ns": write_phase, "write_and_flush_service_elapsed_ns": write_service,
                "desktop_elapsed_ms": 1000.0,
                "sample_count": 1, "run_count": 1, "required_run_count": 1, "passes": 1, "failures": 0,
                "started_at": started.isoformat().replace("+00:00", "Z"), "finished_at": finished.isoformat().replace("+00:00", "Z"),
                "outcome": "completed", "pass": True, "valid": True, "invalid_reason": "", "evidence_paths": ["share/nvme-result.json", "power-source.log"]}
            defaults = {"read_phase_mib_per_sec": read_rate, "read_service_mib_per_sec": 125.0,
                "read_throughput_mib_s": read_rate, "read_p50_ms": 1.0, "read_p95_ms": 4.0,
                "read_p99_ms": read_p99, "read_max_ms": 6.0,
                "write_durable_mib_per_sec": 80.0, "write_and_flush_service_mib_per_sec": 100.0,
                "write_durable_throughput_mib_s": 80.0, "write_p50_ms": 1.0,
                "write_p95_ms": 2.0, "write_p99_ms": 3.0, "write_max_ms": 4.0,
                "flush_p50_ms": 1.0, "flush_p95_ms": 2.0, "flush_p99_ms": 2.0,
                "flush_max_ms": 2.0}
            receipt.update(defaults)
            if candidate and guardrail_regression: receipt["desktop_elapsed_ms"] = 5000.0
            _private_fixture(directory, receipt, receipt["nonce"], candidate)
    campaign(AA_FIXTURE_ID, "AA", 1); campaign(AB_FIXTURE_ID, "AB", 2)


def _edit(root: Path, job: str, edit: Callable[[dict[str, Any]], None]) -> None:
    path = root / "done" / job / "receipt.public.json"; data = json.loads(path.read_text()); edit(data); path.write_text(json.dumps(data))


def self_test() -> None:
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary); _fixture(root); report = analyze(root, AA_FIXTURE_ID, AB_FIXTURE_ID, seal_analyzer=False)
        assert report["claim_eligible"] and report["aa"]["pairs"] == 10
        assert report["primary_read_throughput"]["pass"] and report["aa"]["metrics"]["read_throughput_mib_s"]["bootstrap_samples"] == 10_000
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary); _fixture(root, guardrail_regression=True)
        assert not analyze(root, AA_FIXTURE_ID, AB_FIXTURE_ID, seal_analyzer=False)["claim_eligible"]
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary); _fixture(root); shutil.rmtree(root / "done" / f"t16-{AA_FIXTURE_ID}-020")
        try: analyze(root, AA_FIXTURE_ID, AB_FIXTURE_ID, seal_analyzer=False)
        except EvidenceError as exc: assert "do not exactly cover" in " ".join(exc.errors)
        else: raise AssertionError("incomplete campaign was accepted")
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary); _fixture(root); first = f"t16-{AA_FIXTURE_ID}-001"; second = f"t16-{AA_FIXTURE_ID}-002"
        reused = json.loads((root / "done" / first / "receipt.public.json").read_text())["nonce"]; _edit(root, second, lambda data: data.__setitem__("nonce", reused))
        try: analyze(root, AA_FIXTURE_ID, AB_FIXTURE_ID, seal_analyzer=False)
        except EvidenceError as exc: assert "exact redaction" in " ".join(exc.errors)
        else: raise AssertionError("rewritten public receipt was accepted")
    print("HVF NVMe performance campaign reporter self-test: PASS")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--queue-root", type=Path, default=Path.home() / "BridgeVM" / "live-queue")
    parser.add_argument("--aa-campaign"); parser.add_argument("--ab-campaign"); parser.add_argument("--output", type=Path)
    parser.add_argument("--validate", action="store_true"); parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test(); return 0
    if not args.aa_campaign: parser.error("--aa-campaign is required")
    try: result = analyze(args.queue_root, args.aa_campaign, args.ab_campaign)
    except EvidenceError as exc:
        print(json.dumps(exc.document(), indent=2, sort_keys=True), file=sys.stderr); return 1
    if args.validate:
        print("HVF NVMe performance campaigns: valid"); return 0
    rendered = json.dumps(result, indent=2, sort_keys=True) + "\n"
    if args.output:
        if args.output.exists() or args.output.is_symlink(): raise EvidenceError(["report output must not already exist"])
        with args.output.open("x", encoding="utf-8") as stream: stream.write(rendered)
    else: sys.stdout.write(rendered)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
