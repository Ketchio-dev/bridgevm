#!/usr/bin/env python3
"""Authenticate and report one exact 24-pair/48-lane T16 NVMe v2 A/A campaign."""

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

ROOT = Path(__file__).resolve().parent.parent
TIER = "t16-hvf-nvme-performance"
PROFILE = "windows-nvme-warm-seq-v2"
LANE_SCHEMA = "bridgevm.t16-nvme-calibration-lane.v2"
REGISTRY_SCHEMA = "bridgevm.t16-nvme-calibration-registry.v2"
REPORT_SCHEMA = "bridgevm.t16-nvme-calibration-report.v2"
EXPECTED_RUNS, EXPECTED_PAIRS, BOOTSTRAP_SAMPLES = 48, 24, 10_000
PRIMARY_METRIC, PRIMARY_STOP_PERCENT = "read_phase_mib_per_sec", 2.94
RESOURCE_KEYS = {
    "image", "vars", "binary", "firmware", "renderer", "config", "workload_script",
    "quiescence_script", "quiescence_config", "environment_policy", "campaign_registry", "public_seed",
}
META_KEYS = {
    "schema", "campaign_id", "campaign_mode", "campaign_label", "campaign_order", "campaign_pair",
    "campaign_ordinal", "campaign_expected_runs", "harness_commit", "workload_nonce", "workload_profile",
    "file_mib", "transfer_kib", "read_passes", "write_passes", "queue_depth",
    "post_warmup_settle_seconds", "verification_timing", "replacement_policy", "optional_stopping",
}
FIXED_META = {
    "schema": LANE_SCHEMA, "campaign_mode": "AA", "campaign_expected_runs": "48",
    "workload_profile": PROFILE, "file_mib": "2048", "transfer_kib": "128", "read_passes": "16",
    "write_passes": "4", "queue_depth": "1", "post_warmup_settle_seconds": "30",
    "verification_timing": "outside-timed-read", "replacement_policy": "forbidden",
    "optional_stopping": "forbidden",
}
METRICS = {
    "read_phase_mib_per_sec": {"direction": "higher", "role": "primary"},
    "read_p99_ms": {"direction": "lower", "role": "read-tail-guardrail"},
    "write_durable_throughput_mib_s": {"direction": "higher", "role": "write-guardrail"},
    "write_p99_ms": {"direction": "lower", "role": "write-tail-guardrail"},
    "flush_max_ms": {"direction": "lower", "role": "flush-guardrail"},
    "desktop_elapsed_ms": {"direction": "lower", "role": "desktop-guardrail"},
}
COMMON_FIELDS = (
    "harness_commit", "binary_hash", "image_sha256", "vars_sha256", "firmware_sha256",
    "renderer_sha256", "config_sha256", "public_seed_sha256", "guest_quiescence_script_sha256",
    "guest_quiescence_config_sha256", "environment_policy_sha256", "environment_helper_sha256",
    "pmset_policy_sha256", "host_model", "macos_version", "workload_profile", "file_mib", "transfer_kib",
    "read_passes", "write_passes", "queue_depth", "smp_cpus", "ram_mib", "known_confounders",
    "post_clone_cooldown_seconds", "guest_settle_seconds", "caffeinated", "security_services_enabled",
)
ANALYZER_FILES = (
    "scripts/hvf_nvme_performance_v2_report.py", "scripts/write-hvf-nvme-performance-v2-receipt.py",
    "scripts/render-hvf-nvme-workload-v2.py", "scripts/verify-hvf-nvme-quiescence-v2.py",
    "scripts/live-gates/hvf-nvme-performance-v2-environment.py", "scripts/live-gates/redact-receipt.py",
)


def _module(name: str, path: Path) -> Any:
    specification = importlib.util.spec_from_file_location(name, path)
    if specification is None or specification.loader is None: raise RuntimeError(f"cannot import {path}")
    module = importlib.util.module_from_spec(specification); specification.loader.exec_module(module); return module


WRITER = _module("bridgevm_t16_v2_writer", ROOT / "scripts/write-hvf-nvme-performance-v2-receipt.py")
RENDERER = _module("bridgevm_t16_v2_renderer", ROOT / "scripts/render-hvf-nvme-workload-v2.py")
REDACTOR = _module("bridgevm_receipt_redactor_v2", ROOT / "scripts/live-gates/redact-receipt.py")


class EvidenceError(Exception):
    def __init__(self, errors: list[str], attempts: list[dict[str, Any]] | None = None):
        super().__init__("; ".join(errors)); self.errors, self.attempts = errors, attempts or []

    def document(self) -> dict[str, Any]:
        return {"schema": REPORT_SCHEMA, "error": "campaign evidence is incomplete or invalid",
                "errors": self.errors, "attempts": self.attempts, "claim_eligible": False}


def _hex(value: Any, width: int) -> bool:
    return isinstance(value, str) and bool(re.fullmatch(rf"[0-9a-f]{{{width}}}", value))


def _integer(value: Any, name: str, low: int = 1, high: int | None = None) -> int:
    if not isinstance(value, str) or not value.isascii() or not value.isdigit() or str(int(value)) != value:
        raise ValueError(f"{name} must be canonical decimal")
    result = int(value)
    if result < low or (high is not None and result > high): raise ValueError(f"{name} is outside its fixed range")
    return result


def _timestamp(value: Any, name: str) -> datetime:
    if not isinstance(value, str): raise ValueError(f"{name} must be an ISO-8601 timestamp")
    try: parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError as exc: raise ValueError(f"{name} must be an ISO-8601 timestamp") from exc
    if parsed.tzinfo is None: raise ValueError(f"{name} must have a timezone")
    return parsed


def _regular(path: Path, label: str, limit: int = 8 * 1024 * 1024) -> bytes:
    if not path.is_file() or path.is_symlink(): raise ValueError(f"{label} must be a regular non-symlink file")
    payload = path.read_bytes()
    if not payload or len(payload) >= limit: raise ValueError(f"{label} violates its size boundary")
    return payload


def _signature(path: Path) -> tuple[int, int, int, int, int]:
    status = path.stat(); return status.st_dev, status.st_ino, status.st_size, status.st_mtime_ns, status.st_ctime_ns


def _file_hash(path: Path, label: str, cache: dict[Path, tuple[tuple[int, int, int, int, int], str]] | None = None) -> str:
    if not path.is_file() or path.is_symlink() or path.stat().st_size < 1:
        raise ValueError(f"{label} must be a nonempty regular non-symlink file")
    resolved, before = path.resolve(), _signature(path)
    if cache is not None and resolved in cache:
        cached_signature, cached_hash = cache[resolved]
        if before != cached_signature: raise ValueError(f"{label} changed during report authentication")
        return cached_hash
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""): digest.update(block)
    if _signature(path) != before: raise ValueError(f"{label} changed while it was hashed")
    result = digest.hexdigest()
    if cache is not None: cache[resolved] = (before, result)
    return result


def _json(path: Path, label: str) -> dict[str, Any]:
    payload = _regular(path, label)
    def unique(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
        result: dict[str, Any] = {}
        for key, value in pairs:
            if key in result: raise ValueError(f"{label} contains duplicate JSON key {key!r}")
            result[key] = value
        return result
    try: document = json.loads(payload.decode("utf-8"), object_pairs_hook=unique)
    except (UnicodeError, json.JSONDecodeError) as exc: raise ValueError(f"{label} is not UTF-8 JSON") from exc
    if not isinstance(document, dict): raise ValueError(f"{label} must contain one JSON object")
    return document


def _rows(path: Path) -> list[list[str]]:
    payload = _regular(path, path.name, 1024 * 1024)
    try: text = payload.decode("utf-8")
    except UnicodeError as exc: raise ValueError(f"{path.name} is not UTF-8") from exc
    if "\r" in text or not text.endswith("\n") or "\n\n" in text: raise ValueError(f"{path.name} has non-canonical lines")
    return [line.split("\t") for line in text.splitlines()]


def _env(path: Path) -> dict[str, str]:
    result: dict[str, str] = {}
    for row in _regular(path, path.name, 64 * 1024).decode("utf-8").splitlines():
        key, separator, value = row.partition("=")
        if not separator or not key or key in result: raise ValueError(f"{path.name} is malformed or duplicated")
        result[key] = value
    return result


def _expected_nonce(seed: str, campaign_id: str, ordinal: int) -> str:
    return hashlib.sha256(f"{seed}:{campaign_id}:{ordinal}".encode()).hexdigest()[:32]


def _orders(seed: str) -> dict[int, str]:
    ranked = sorted((hashlib.sha256(f"{seed}:{pair}".encode()).hexdigest(), pair) for pair in range(1, 25))
    ab = {pair for _, pair in ranked[:12]}; return {pair: "AB" if pair in ab else "BA" for pair in range(1, 25)}


def _manifest(path: Path) -> tuple[dict[str, str], dict[str, tuple[Path, str]]]:
    metadata: dict[str, str] = {}; resources: dict[str, tuple[Path, str]] = {}
    rows = _rows(path)
    for row in rows:
        key = row[0] if row else ""
        if key in metadata or key in resources: raise ValueError(f"duplicate manifest key {key!r}")
        if key in RESOURCE_KEYS and len(row) == 3 and Path(row[1]).is_absolute() and _hex(row[2], 64):
            resources[key] = (Path(row[1]), row[2])
        elif key in META_KEYS and len(row) == 2 and row[1]: metadata[key] = row[1]
        else: raise ValueError(f"invalid manifest row {key!r}")
    if len(rows) != len(RESOURCE_KEYS) + len(META_KEYS) or set(resources) != RESOURCE_KEYS or set(metadata) != META_KEYS:
        raise ValueError("manifest is not the exact v2 lane schema")
    if any(metadata.get(key) != value for key, value in FIXED_META.items()):
        raise ValueError("manifest mixes a non-v2 profile, geometry, or stopping policy")
    if not _hex(metadata["campaign_id"], 32) or not _hex(metadata["harness_commit"], 40) or not _hex(metadata["workload_nonce"], 32):
        raise ValueError("manifest campaign/source/nonce identity is not canonical")
    ordinal = _integer(metadata["campaign_ordinal"], "campaign ordinal", 1, 48)
    pair = _integer(metadata["campaign_pair"], "campaign pair", 1, 24)
    order, label = metadata["campaign_order"], metadata["campaign_label"]
    if pair != (ordinal + 1) // 2 or order not in {"AB", "BA"} or label != order[0 if ordinal % 2 else 1]:
        raise ValueError("manifest pair/order/label is invalid")
    if resources["config"][1] != WRITER.CONFIG_SHA256 or resources["quiescence_config"][1] != WRITER.QUIESCENCE_CONFIG_SHA256:
        raise ValueError("manifest does not seal the registered workload/quiescence config")
    return metadata, resources


def _registry(root: Path, campaign_id: str) -> dict[str, Any]:
    campaign_root = root / "campaign-inputs-v2" / campaign_id
    path, seed_path = campaign_root / "campaign-registry.tsv", campaign_root / "public-seed.txt"
    seed_bytes = _regular(seed_path, "public seed", 1024)
    if not re.fullmatch(rb"[0-9a-f]{64}\n", seed_bytes): raise ValueError("public seed is not exact 64-hex plus LF")
    seed = seed_bytes[:-1].decode("ascii"); rows = _rows(path)
    if len(rows) != 21 + EXPECTED_RUNS: raise ValueError("registry must have exact v2 headers and 48 lanes")
    headers = rows[:21]
    if any(len(row) != 2 for row in headers) or len({row[0] for row in headers}) != len(headers):
        raise ValueError("registry header is malformed or duplicated")
    values = {row[0]: row[1] for row in headers}; harness = values.get("harness_commit", "")
    binary = values.get("binary_a_sha256", "")
    expected_headers = [
        ["schema", REGISTRY_SCHEMA], ["campaign_id", campaign_id], ["campaign_mode", "AA"],
        ["workload_profile", PROFILE], ["pairs", "24"], ["expected_runs", "48"], ["harness_commit", harness],
        ["public_seed", seed], ["public_seed_sha256", hashlib.sha256(seed_bytes).hexdigest()],
        ["binary_a_sha256", binary], ["binary_b_sha256", binary], ["order_algorithm", "sha256-balanced-rank-v1"],
        ["replacement_policy", "forbidden"], ["optional_stopping", "forbidden"], ["file_mib", "2048"],
        ["transfer_kib", "128"], ["read_passes", "16"], ["write_passes", "4"], ["queue_depth", "1"],
        ["post_warmup_settle_seconds", "30"], ["verification_timing", "outside-timed-read"],
    ]
    if headers != expected_headers or not _hex(harness, 40) or not _hex(binary, 64):
        raise ValueError("registry header is not the exact separately-versioned v2 contract")
    orders = _orders(seed); lanes: dict[int, dict[str, Any]] = {}
    for ordinal, row in enumerate(rows[21:], 1):
        pair, index = (ordinal + 1) // 2, 0 if ordinal % 2 else 1; order = orders[pair]; label = order[index]
        expected = ["lane", str(ordinal), str(pair), order, label, binary, f"t16-{campaign_id}-{ordinal:03d}"]
        if row != expected: raise ValueError(f"registry lane {ordinal} is not the public-seed schedule")
        lanes[ordinal] = {"pair": pair, "order": order, "label": label, "binary": binary, "job_id": expected[-1]}
    return {"root": campaign_root, "path": path.resolve(), "path_hash": hashlib.sha256(path.read_bytes()).hexdigest(),
            "seed_path": seed_path.resolve(), "seed_hash": hashlib.sha256(seed_bytes).hexdigest(), "seed": seed,
            "harness": harness, "binary": binary, "lanes": lanes}


def _manifest_resources(resources: dict[str, tuple[Path, str]],
                        cache: dict[Path, tuple[tuple[int, int, int, int, int], str]]) -> None:
    for key, (path, expected_hash) in resources.items():
        if key in {"image", "vars"}:
            if not path.is_file() or path.is_symlink() or path.stat().st_size < 1:
                raise ValueError(f"manifest resource {key} is no longer a safe independent lane file")
            resolved, signature = path.resolve(), _signature(path)
            if resolved in cache and cache[resolved][0] != signature:
                raise ValueError(f"manifest resource {key} changed during report authentication")
            cache[resolved] = (signature, expected_hash)
        elif _file_hash(path, f"manifest resource {key}", cache) != expected_hash:
            raise ValueError(f"manifest resource {key} bytes changed")
    RENDERER.verify_output(resources["workload_script"][0], resources["config"][0])


def _authenticated_receipt(directory: Path, resources: dict[str, tuple[Path, str]]) -> dict[str, Any]:
    private = _json(directory / "receipt.json", "private receipt")
    public = _json(directory / "receipt.public.json", "public receipt")
    if public != REDACTOR.redact(private): raise ValueError("public receipt is not exact dedicated redaction")
    if public != private or set(public) != WRITER.RECEIPT_KEYS:
        raise ValueError("public receipt omits an authenticated v2 aggregate field")
    share = directory / "share"
    result, hashes = WRITER.load_artifacts(share / "nvme-result.json", share / "nvme-raw.json",
                                           share / "nvme-result.done", share / "nvme-workload-config.json")
    evidence_hashes, environment = WRITER.load_environment(
        directory / "power-source.log", resources["environment_policy"][0], ROOT / "scripts/live-gates/hvf-nvme-performance-v2-environment.py",
        directory / "pmset-policy.txt", directory / "thermal.json", directory / "hid.json",
        share / "bv-nvme-quiescence-v2.ps1", share / "hvf-nvme-performance-v2-quiescence.json",
        share / "nvme-quiescence-result.json", result["nonce"])
    WRITER.validate_receipt(private, result, {**hashes, **evidence_hashes}, environment)
    return private


def _validate_job(root: Path, job: dict[str, Any], registry: dict[str, Any],
                  cache: dict[Path, tuple[tuple[int, int, int, int, int], str]]) -> None:
    metadata, resources = _manifest(job["manifest_path"]); job["metadata"], job["resources"] = metadata, resources
    ordinal = int(metadata["campaign_ordinal"]); lane = registry["lanes"].get(ordinal)
    if metadata["campaign_id"] != registry["path"].parent.name or metadata["harness_commit"] != registry["harness"]:
        raise ValueError("manifest campaign/harness disagrees with registry")
    if lane is None or (metadata["campaign_pair"], metadata["campaign_order"], metadata["campaign_label"], job["job_id"]) \
            != (str(lane["pair"]), lane["order"], lane["label"], lane["job_id"]):
        raise ValueError("job does not match its immutable registry lane")
    if (resources["campaign_registry"][0].resolve(), resources["campaign_registry"][1]) != (registry["path"], registry["path_hash"]):
        raise ValueError("manifest does not seal the canonical registry")
    if (resources["public_seed"][0].resolve(), resources["public_seed"][1]) != (registry["seed_path"], registry["seed_hash"]):
        raise ValueError("manifest does not seal the canonical public seed")
    if metadata["workload_nonce"] != _expected_nonce(registry["seed"], metadata["campaign_id"], ordinal):
        raise ValueError("manifest nonce is not bound to public seed/campaign/ordinal")
    _manifest_resources(resources, cache)
    if job["state"] != "done": raise ValueError(f"attempt remains in {job['state']} state")
    directory = job["manifest_path"].parent
    job_env, result_env = _env(directory / "job.env"), _env(directory / "result.env")
    if result_env.get("result") != "pass" or result_env.get("exit_code") != "0":
        raise ValueError("worker did not publish one successful zero-exit lane")
    manifest_hash = hashlib.sha256(job["manifest_path"].read_bytes()).hexdigest()
    immutable = {"job_id": job["job_id"], "tier": TIER, "commit": registry["harness"],
                 "input_manifest_sha256": manifest_hash, "sealed_binary_sha256": registry["binary"]}
    if set(job_env) != {*immutable, "submitted_at", "started_at", "finished_at"} \
            or any(job_env.get(field) != value for field, value in immutable.items()):
        raise ValueError("job.env is not the exact sealed lane identity plus worker timestamps")
    submitted = _timestamp(job_env["submitted_at"], "job submitted_at")
    worker_started = _timestamp(job_env["started_at"], "job started_at")
    worker_finished = _timestamp(job_env["finished_at"], "job finished_at")
    if not submitted <= worker_started < worker_finished:
        raise ValueError("job.env timestamps are not ordered")
    ledger = _env(root / "job-ledger" / job["job_id"] / "entry.env")
    if ledger != immutable: raise ValueError("durable submission ledger does not match immutable job.env identity")
    receipt = _authenticated_receipt(directory, resources); job["receipt"] = receipt
    expected_receipt = {
        "job_id": job["job_id"], "commit": registry["harness"], "harness_commit": registry["harness"],
        "input_manifest_sha256": manifest_hash, "campaign_id": metadata["campaign_id"], "campaign_mode": "AA",
        "campaign_label": metadata["campaign_label"], "campaign_order": metadata["campaign_order"],
        "campaign_pair": int(metadata["campaign_pair"]), "campaign_ordinal": ordinal, "campaign_expected_runs": 48,
        "nonce": metadata["workload_nonce"], "binary_hash": resources["binary"][1], "image_sha256": resources["image"][1],
        "vars_sha256": resources["vars"][1], "firmware_sha256": resources["firmware"][1],
        "renderer_sha256": resources["renderer"][1], "config_sha256": resources["config"][1],
        "workload_script_sha256": resources["workload_script"][1], "campaign_registry_sha256": resources["campaign_registry"][1],
        "public_seed_sha256": resources["public_seed"][1],
        "guest_quiescence_script_sha256": resources["quiescence_script"][1],
        "guest_quiescence_config_sha256": resources["quiescence_config"][1],
        "environment_policy_sha256": resources["environment_policy"][1],
        "environment_helper_sha256": _file_hash(ROOT / "scripts/live-gates/hvf-nvme-performance-v2-environment.py", "environment helper", cache),
    }
    for field, expected in expected_receipt.items():
        if receipt.get(field) != expected: raise ValueError(f"receipt {field} disagrees with sealed campaign input")
    started, finished = _timestamp(receipt["started_at"], "started_at"), _timestamp(receipt["finished_at"], "finished_at")
    if finished <= started or not worker_started <= started < finished <= worker_finished:
        raise ValueError("receipt timestamps are not increasing within the worker interval")
    job["started"], job["finished"] = started, finished


def _mentions(path: Path, campaign_id: str) -> bool:
    try: return f"campaign_id\t{campaign_id}".encode() in path.read_bytes().splitlines()
    except OSError: return False


def _discover(root: Path, campaign_id: str) -> list[dict[str, Any]]:
    jobs: list[dict[str, Any]] = []
    for state in ("queued", "running", "done"):
        state_root = root / state
        if not state_root.is_dir(): continue
        for path in sorted(state_root.glob("*/input-manifest.tsv")):
            if _mentions(path, campaign_id): jobs.append({"job_id": path.parent.name, "state": state, "manifest_path": path})
    return jobs


def _attempt(job: dict[str, Any]) -> dict[str, Any]:
    metadata, receipt = job.get("metadata", {}), job.get("receipt", {})
    return {"job_id": job["job_id"], "state": job["state"], "ordinal": metadata.get("campaign_ordinal"),
            "pair": metadata.get("campaign_pair"), "label": metadata.get("campaign_label"),
            "pass": receipt.get("pass"), "invalid_reason": receipt.get("invalid_reason")}


def load_campaign(root: Path, campaign_id: str) -> dict[str, Any]:
    if not _hex(campaign_id, 32): raise EvidenceError(["campaign id must be 32 lowercase hex characters"])
    try: registry = _registry(root, campaign_id)
    except (OSError, UnicodeError, ValueError) as exc: raise EvidenceError([f"campaign {campaign_id}: {exc}"]) from exc
    jobs, errors = _discover(root, campaign_id), []
    hash_cache: dict[Path, tuple[tuple[int, int, int, int, int], str]] = {}
    for job in jobs:
        try: _validate_job(root, job, registry, hash_cache)
        except (OSError, UnicodeError, json.JSONDecodeError, ValueError) as exc: errors.append(f"{job['job_id']}: {exc}")
    attempts = [_attempt(job) for job in jobs]
    expected_ids = {lane["job_id"] for lane in registry["lanes"].values()}
    ids = [job["job_id"] for job in jobs]
    if len(ids) != len(set(ids)): errors.append("campaign contains duplicate job IDs")
    if set(ids) != expected_ids or len(ids) != EXPECTED_RUNS:
        errors.append("registered job IDs do not exactly equal all 48 discovered attempts; replacement/trimming is forbidden")
    valid = sorted((job for job in jobs if "started" in job), key=lambda item: int(item["metadata"]["campaign_ordinal"]))
    if [int(job["metadata"]["campaign_ordinal"]) for job in valid] != list(range(1, 49)):
        errors.append("valid lanes do not exactly cover ordinals 1..48")
    for previous, current in zip(valid, valid[1:]):
        if current["started"] < previous["finished"]: errors.append(f"ordinal {current['metadata']['campaign_ordinal']} overlaps its predecessor")
    if errors: raise EvidenceError(errors, attempts)
    reference = valid[0]["receipt"]
    for job in valid[1:]:
        for field in COMMON_FIELDS:
            if job["receipt"].get(field) != reference.get(field): errors.append(f"campaign mismatches common identity {field}")
    for field in ("nonce", "workload_script_sha256", "raw_sha256", "result_sha256", "done_sha256", "guest_quiescence_log_sha256"):
        values = [job["receipt"][field] for job in valid]
        if len(values) != len(set(values)): errors.append(f"campaign reuses per-lane evidence identity {field}")
    media = [(job["resources"][key][0].stat().st_dev, job["resources"][key][0].stat().st_ino)
             for job in valid for key in ("image", "vars")]
    if len(media) != 96 or len(set(media)) != 96: errors.append("campaign does not retain 48 independent disk and vars source lanes")
    if len({job["receipt"]["binary_hash"] for job in valid}) != 1 or reference["binary_hash"] != registry["binary"]:
        errors.append("A/A labels do not use one identical sealed binary")
    for path, (signature, _) in hash_cache.items():
        try:
            if _signature(path) != signature: errors.append(f"authenticated resource changed during report: {path.name}")
        except OSError:
            errors.append(f"authenticated resource disappeared during report: {path.name}")
    if errors: raise EvidenceError(errors, attempts)
    return {"id": campaign_id, "registry": registry, "jobs": valid, "attempts": attempts}


def _paired(campaign: dict[str, Any], metric: str) -> tuple[list[float], list[float], list[float]]:
    label_a: list[float] = []; label_b: list[float] = []
    for pair in range(1, EXPECTED_PAIRS + 1):
        jobs = [job for job in campaign["jobs"] if int(job["metadata"]["campaign_pair"]) == pair]
        if len(jobs) != 2: raise ValueError(f"pair {pair} does not contain exactly two lanes")
        a = next(job for job in jobs if job["metadata"]["campaign_label"] == "A")
        b = next(job for job in jobs if job["metadata"]["campaign_label"] == "B")
        label_a.append(float(a["receipt"][metric])); label_b.append(float(b["receipt"][metric]))
    higher = METRICS[metric]["direction"] == "higher"
    deltas = [100.0 * ((b - a) if higher else (a - b)) / a for a, b in zip(label_a, label_b)]
    return label_a, label_b, deltas


def _interval(deltas: list[float], metric: str) -> tuple[float, float, float]:
    if len(deltas) != EXPECTED_PAIRS: raise ValueError("bootstrap requires every one of the exact 24 pairs")
    seed = int.from_bytes(hashlib.sha256(f"bridgevm:t16:nvme:v2:AA:{metric}:10000".encode()).digest()[:8], "big")
    rng = random.Random(seed)
    bootstrap = sorted(statistics.median(rng.choices(deltas, k=EXPECTED_PAIRS)) for _ in range(BOOTSTRAP_SAMPLES))
    return statistics.median(deltas), bootstrap[249], bootstrap[9749]


def _stop_decision(bound: float) -> bool:
    return math.isfinite(bound) and bound < PRIMARY_STOP_PERCENT


def _seal_analyzer(commit: str) -> dict[str, Any]:
    head = subprocess.check_output(["git", "-C", str(ROOT), "rev-parse", "HEAD"], text=True).strip()
    if head != commit: raise ValueError("reporter checkout does not equal the sealed campaign commit")
    dirty = subprocess.check_output(["git", "-C", str(ROOT), "status", "--porcelain", "--untracked-files=all", "--", *ANALYZER_FILES], text=True)
    if dirty: raise ValueError("reporter or an authenticated dependency has uncommitted bytes")
    hashes: dict[str, str] = {}
    for relative in ANALYZER_FILES:
        current = (ROOT / relative).read_bytes(); committed = subprocess.check_output(["git", "-C", str(ROOT), "show", f"{commit}:{relative}"])
        if current != committed: raise ValueError(f"analyzer dependency differs from {commit}:{relative}")
        hashes[relative] = hashlib.sha256(current).hexdigest()
    return {"harness_commit": commit, "source_sha256": hashes}


def analyze(root: Path, campaign_id: str, *, seal_analyzer: bool = True) -> dict[str, Any]:
    root = root.resolve(strict=True); campaign = load_campaign(root, campaign_id)
    metrics: dict[str, Any] = {}
    for metric, definition in METRICS.items():
        a, b, deltas = _paired(campaign, metric); estimate, lower, upper = _interval(deltas, metric)
        metrics[metric] = {"direction": definition["direction"], "role": definition["role"], "label_a_samples": a,
                           "label_b_samples": b, "paired_directional_delta_percent": estimate,
                           "paired_directional_delta_percent_ci95": [lower, upper],
                           "aa_absolute_ci_bound_percent": max(abs(lower), abs(upper)),
                           "bootstrap_samples": BOOTSTRAP_SAMPLES,
                           "bootstrap_seed_rule": f"sha256(bridgevm:t16:nvme:v2:AA:{metric}:10000)[0:8]"}
    primary_bound = metrics[PRIMARY_METRIC]["aa_absolute_ci_bound_percent"]; passed = _stop_decision(primary_bound)
    result: dict[str, Any] = {
        "schema": REPORT_SCHEMA, "campaign_id": campaign_id, "campaign_mode": "AA", "workload_profile": PROFILE,
        "expected_runs": EXPECTED_RUNS, "valid_runs": EXPECTED_RUNS, "pairs": EXPECTED_PAIRS,
        "replacement_policy": "forbidden", "optional_stopping": "forbidden", "trimmed_lanes": 0,
        "metrics": metrics, "primary_stop_rule": {"metric": PRIMARY_METRIC,
            "rule": "two-sided 95% paired-median A/A absolute CI bound must be strictly below 2.94 percent",
            "observed_bound_percent": primary_bound, "threshold_percent": PRIMARY_STOP_PERCENT,
            "comparison": "<", "pass": passed},
        "candidate_diagnostic_permitted": passed, "claim_eligible": False,
        "claim_reason": "A/A calibration only; no A/A result can support a product performance claim",
        "attempts": campaign["attempts"],
    }
    if seal_analyzer:
        try: result["analysis_identity"] = _seal_analyzer(campaign["registry"]["harness"])
        except (OSError, subprocess.SubprocessError, ValueError) as exc: raise EvidenceError([f"analyzer is not sealed: {exc}"], campaign["attempts"]) from exc
    return result


def _fixture(root: Path) -> str:
    campaign_id, seed, harness, binary_hash = "0" * 32, "42" * 32, "1" * 40, "2" * 64
    campaign_root = root / "campaign-inputs-v2" / campaign_id; campaign_root.mkdir(parents=True)
    seed_path = campaign_root / "public-seed.txt"; seed_path.write_text(seed + "\n", encoding="ascii")
    orders = _orders(seed); registry_path = campaign_root / "campaign-registry.tsv"
    headers = [
        ["schema", REGISTRY_SCHEMA], ["campaign_id", campaign_id], ["campaign_mode", "AA"], ["workload_profile", PROFILE],
        ["pairs", "24"], ["expected_runs", "48"], ["harness_commit", harness], ["public_seed", seed],
        ["public_seed_sha256", hashlib.sha256(seed_path.read_bytes()).hexdigest()], ["binary_a_sha256", binary_hash],
        ["binary_b_sha256", binary_hash], ["order_algorithm", "sha256-balanced-rank-v1"],
        ["replacement_policy", "forbidden"], ["optional_stopping", "forbidden"], ["file_mib", "2048"],
        ["transfer_kib", "128"], ["read_passes", "16"], ["write_passes", "4"], ["queue_depth", "1"],
        ["post_warmup_settle_seconds", "30"], ["verification_timing", "outside-timed-read"],
    ]
    rows = headers[:]
    for ordinal in range(1, 49):
        pair, index = (ordinal + 1) // 2, 0 if ordinal % 2 else 1; order = orders[pair]
        rows.append(["lane", str(ordinal), str(pair), order, order[index], binary_hash, f"t16-{campaign_id}-{ordinal:03d}"])
    registry_path.write_text("\n".join("\t".join(row) for row in rows) + "\n")
    registry_hash, seed_hash = hashlib.sha256(registry_path.read_bytes()).hexdigest(), hashlib.sha256(seed_path.read_bytes()).hexdigest()
    common = campaign_root / "common"; common.mkdir(); binary = common / "binary"; binary.write_bytes(b"same AA binary\n")
    binary_hash = hashlib.sha256(binary.read_bytes()).hexdigest()
    # Rewrite the registry once with the real identical A/B binary identity.
    for row in rows:
        if row[0] in {"binary_a_sha256", "binary_b_sha256"}: row[1] = binary_hash
        if row[0] == "lane": row[5] = binary_hash
    registry_path.write_text("\n".join("\t".join(row) for row in rows) + "\n"); registry_hash = hashlib.sha256(registry_path.read_bytes()).hexdigest()
    firmware = common / "firmware"; firmware.write_bytes(b"firmware\n")
    renderer_library = common / "libvirglrenderer.dylib"; renderer_library.write_bytes(b"renderer library\n")
    variations = [-.8, -.6, -.4, -.2, 0, .2, .4, .6] * 3
    for ordinal in range(1, 49):
        pair, order = (ordinal + 1) // 2, orders[(ordinal + 1) // 2]; label = order[0 if ordinal % 2 else 1]
        job_id = f"t16-{campaign_id}-{ordinal:03d}"; lane = campaign_root / f"lane-{ordinal}"; lane.mkdir()
        started = datetime(2026, 1, 1, tzinfo=timezone.utc) + timedelta(minutes=2 * (ordinal - 1))
        worker_started, worker_finished = started - timedelta(seconds=15), started + timedelta(minutes=1, seconds=15)
        submitted = worker_started - timedelta(minutes=1)
        image, vars_path = lane / "disk.raw", lane / "vars.fd"; image.write_bytes(b"image\n"); vars_path.write_bytes(b"vars\n")
        nonce = _expected_nonce(seed, campaign_id, ordinal)
        workload, config = lane / "nvme-workload.ps1", lane / "nvme-workload-config.json"
        workload.write_bytes(RENDERER.render(nonce)); config.write_bytes(RENDERER.canonical_config_bytes())
        lane_quiescence_script = lane / "bv-nvme-quiescence-v2.ps1"
        lane_quiescence_config = lane / "hvf-nvme-performance-v2-quiescence.json"
        lane_environment_policy = lane / "hvf-nvme-performance-v2-environment.sh"
        shutil.copyfile(ROOT / "scripts/win-assets/bv-nvme-quiescence-v2.ps1", lane_quiescence_script)
        shutil.copyfile(ROOT / "scripts/live-gates/hvf-nvme-performance-v2-quiescence.json", lane_quiescence_config)
        shutil.copyfile(ROOT / "scripts/live-gates/hvf-nvme-performance-v2-environment.sh", lane_environment_policy)
        resources = {
            "image": image, "vars": vars_path, "binary": binary, "firmware": firmware,
            "renderer": renderer_library, "config": config, "workload_script": workload,
            "quiescence_script": lane_quiescence_script, "quiescence_config": lane_quiescence_config,
            "environment_policy": lane_environment_policy,
            "campaign_registry": registry_path, "public_seed": seed_path,
        }
        metadata = {**FIXED_META, "campaign_id": campaign_id, "campaign_label": label, "campaign_order": order,
                    "campaign_pair": str(pair), "campaign_ordinal": str(ordinal), "harness_commit": harness,
                    "workload_nonce": nonce}
        manifest_rows = [[key, str(path), hashlib.sha256(path.read_bytes()).hexdigest()] for key, path in resources.items()]
        manifest_rows += [[key, value] for key, value in metadata.items()]
        directory = root / "done" / job_id; (directory / "share").mkdir(parents=True)
        manifest = directory / "input-manifest.tsv"; manifest.write_text("\n".join("\t".join(row) for row in manifest_rows) + "\n")
        manifest_hash = hashlib.sha256(manifest.read_bytes()).hexdigest()
        immutable_env = f"job_id={job_id}\ntier={TIER}\ncommit={harness}\ninput_manifest_sha256={manifest_hash}\nsealed_binary_sha256={binary_hash}\n"
        job_env = immutable_env + f"submitted_at={submitted.isoformat().replace('+00:00', 'Z')}\nstarted_at={worker_started.isoformat().replace('+00:00', 'Z')}\nfinished_at={worker_finished.isoformat().replace('+00:00', 'Z')}\n"
        (directory / "job.env").write_text(job_env); (directory / "result.env").write_text("result=pass\nexit_code=0\n")
        ledger = root / "job-ledger" / job_id; ledger.mkdir(parents=True); (ledger / "entry.env").write_text(immutable_env)
        delta = variations[pair - 1] if label == "B" else 0.0
        phase = round(32_000_000_000 / (1 + delta / 100.0))
        result, hashes, evidence_hashes, environment = WRITER._fixture(directory / "private", campaign_id=campaign_id,
                                                                        ordinal=ordinal, read_phase_ns=phase,
                                                                        nonce_override=nonce)
        private_root, share = directory / "private", directory / "share"
        for source, target in ((private_root / "result.json", share / "nvme-result.json"),
                               (private_root / "raw.json", share / "nvme-raw.json"),
                               (private_root / "done.json", share / "nvme-result.done"),
                               (private_root / "config.json", share / "nvme-workload-config.json"),
                               (resources["quiescence_script"], share / "bv-nvme-quiescence-v2.ps1"),
                               (private_root / "quiet-config.json", share / "hvf-nvme-performance-v2-quiescence.json"),
                               (private_root / "quiet.json", share / "nvme-quiescence-result.json"),
                               (private_root / "power.log", directory / "power-source.log"),
                               (private_root / "pmset.txt", directory / "pmset-policy.txt"),
                               (private_root / "thermal.json", directory / "thermal.json"),
                               (private_root / "hid.json", directory / "hid.json")):
            shutil.copyfile(source, target)
        shutil.rmtree(private_root)
        # Re-authenticate the bytes at their final runtime names.
        result, hashes = WRITER.load_artifacts(share / "nvme-result.json", share / "nvme-raw.json", share / "nvme-result.done", share / "nvme-workload-config.json")
        evidence_hashes, environment = WRITER.load_environment(directory / "power-source.log", resources["environment_policy"],
            ROOT / "scripts/live-gates/hvf-nvme-performance-v2-environment.py", directory / "pmset-policy.txt", directory / "thermal.json", directory / "hid.json",
            share / "bv-nvme-quiescence-v2.ps1", share / "hvf-nvme-performance-v2-quiescence.json",
            share / "nvme-quiescence-result.json", nonce)
        identity = {"job_id": job_id, "commit": harness, "harness_commit": harness, "binary_hash": binary_hash,
            "input_manifest_sha256": manifest_hash, "image_sha256": hashlib.sha256(image.read_bytes()).hexdigest(),
            "vars_sha256": hashlib.sha256(vars_path.read_bytes()).hexdigest(), "firmware_sha256": hashlib.sha256(firmware.read_bytes()).hexdigest(),
            "renderer_sha256": hashlib.sha256(resources["renderer"].read_bytes()).hexdigest(), "config_sha256": WRITER.CONFIG_SHA256,
            "campaign_registry_sha256": registry_hash, "public_seed_sha256": seed_hash,
            "workload_script_sha256": hashlib.sha256(workload.read_bytes()).hexdigest(),
            "campaign_id": campaign_id, "campaign_mode": "AA", "campaign_label": label, "campaign_order": order,
            "campaign_pair": pair, "campaign_ordinal": ordinal, "campaign_expected_runs": 48, "host_model": "MacTest",
            "macos_version": "26.0", "smp_cpus": 4, "ram_mib": 6144, "desktop_elapsed_ms": 1000.0,
            "started_at": started.isoformat().replace("+00:00", "Z"),
            "finished_at": (started + timedelta(minutes=1)).isoformat().replace("+00:00", "Z")}
        receipt = WRITER.build_receipt(identity, result, hashes, evidence_hashes, environment)
        (directory / "receipt.json").write_bytes(WRITER._canonical(receipt))
        (directory / "receipt.public.json").write_text(json.dumps(REDACTOR.redact(receipt), sort_keys=True, separators=(",", ":")))
    return campaign_id


def _expect_invalid(action: Callable[[], Any], text: str) -> None:
    try: action()
    except EvidenceError as exc:
        if text not in " ".join(exc.errors): raise AssertionError((text, exc.errors))
    else: raise AssertionError(f"invalid evidence was accepted: {text}")


def self_test() -> None:
    checks = 0
    with tempfile.TemporaryDirectory(prefix="bridgevm-t16-v2-report-") as temporary:
        root = Path(temporary); campaign = _fixture(root); report = analyze(root, campaign, seal_analyzer=False); checks += 1
        assert report["valid_runs"] == 48 and report["pairs"] == 24 and report["claim_eligible"] is False
        assert report["metrics"][PRIMARY_METRIC]["bootstrap_samples"] == 10_000
        assert report["primary_stop_rule"]["pass"] is True and report["candidate_diagnostic_permitted"] is True
        deltas = _paired(load_campaign(root, campaign), PRIMARY_METRIC)[2]
        assert _interval(deltas, PRIMARY_METRIC) == _interval(deltas, PRIMARY_METRIC); checks += 1
        assert _stop_decision(2.939999999) is True and _stop_decision(2.94) is False; checks += 1
        missing = root / "done" / f"t16-{campaign}-048"; backup = root / "missing"; missing.rename(backup)
        _expect_invalid(lambda: analyze(root, campaign, seal_analyzer=False), "exactly equal all 48"); backup.rename(missing); checks += 1
        registry = root / "campaign-inputs-v2" / campaign / "campaign-registry.tsv"; original = registry.read_bytes()
        registry.write_bytes(original.replace(REGISTRY_SCHEMA.encode(), b"bridgevm.t16-campaign-registry.v1", 1))
        _expect_invalid(lambda: analyze(root, campaign, seal_analyzer=False), "separately-versioned v2"); registry.write_bytes(original); checks += 1
        first = root / "done" / f"t16-{campaign}-001" / "receipt.public.json"; original_public = first.read_bytes()
        public = json.loads(original_public); public["claim_eligible"] = True; first.write_text(json.dumps(public))
        _expect_invalid(lambda: analyze(root, campaign, seal_analyzer=False), "exact dedicated redaction"); first.write_bytes(original_public); checks += 1
        ledger = root / "job-ledger" / f"t16-{campaign}-001" / "entry.env"; original_ledger = ledger.read_bytes(); ledger.write_bytes(original_ledger + b"extra=1\n")
        _expect_invalid(lambda: analyze(root, campaign, seal_analyzer=False), "durable submission ledger"); ledger.write_bytes(original_ledger); checks += 1
        job_env_path = root / "done" / f"t16-{campaign}-001" / "job.env"; original_job_env = job_env_path.read_bytes()
        job_values = _env(job_env_path); job_values["finished_at"] = job_values["started_at"]
        job_env_path.write_text("\n".join(f"{key}={value}" for key, value in job_values.items()) + "\n")
        _expect_invalid(lambda: analyze(root, campaign, seal_analyzer=False), "timestamps are not ordered"); job_env_path.write_bytes(original_job_env); checks += 1
        thermal = root / "done" / f"t16-{campaign}-001" / "thermal.json"; original_thermal = thermal.read_bytes(); thermal.write_bytes(original_thermal.replace(b'"thermal_state":0', b'"thermal_state":1', 1))
        _expect_invalid(lambda: analyze(root, campaign, seal_analyzer=False), "thermal"); thermal.write_bytes(original_thermal); checks += 1
        replacement = root / "done" / f"replacement-{campaign}"; shutil.copytree(root / "done" / f"t16-{campaign}-001", replacement)
        _expect_invalid(lambda: analyze(root, campaign, seal_analyzer=False), "does not match its immutable registry lane"); shutil.rmtree(replacement); checks += 1
    print(f"HVF NVMe v2 reporter self-test: PASS ({checks} positive/adversarial checks)")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--queue-root", type=Path, default=Path.home() / "BridgeVM" / "live-queue")
    parser.add_argument("--campaign", "--aa-campaign", dest="campaign"); parser.add_argument("--output", type=Path)
    parser.add_argument("--validate", action="store_true"); parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test: self_test(); return 0
    if not args.campaign: parser.error("--campaign is required")
    try: report = analyze(args.queue_root, args.campaign)
    except EvidenceError as exc:
        print(json.dumps(exc.document(), indent=2, sort_keys=True), file=sys.stderr); return 1
    if args.validate: print("HVF NVMe v2 A/A campaign: valid"); return 0
    rendered = json.dumps(report, indent=2, sort_keys=True) + "\n"
    if args.output:
        if args.output.exists() or args.output.is_symlink(): parser.error("--output must not already exist")
        with args.output.open("x", encoding="utf-8") as stream: stream.write(rendered)
    else: sys.stdout.write(rendered)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
