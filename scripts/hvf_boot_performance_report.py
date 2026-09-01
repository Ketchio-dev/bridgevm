"""Strict evidence validation and statistics for sealed HVF boot campaigns."""

from __future__ import annotations

import json
import hashlib
import math
import random
import re
import shutil
import statistics
import tempfile
from datetime import datetime, timedelta, timezone
from pathlib import Path
from typing import Any, Callable

RESOURCE_KEYS = {"image", "vars", "binary"}
META_KEYS = {
    "binary_source_commit", "binary_profile", "binary_features", "rust_toolchain",
    "campaign_id", "campaign_mode", "campaign_role", "campaign_ordinal",
    "campaign_expected_runs",
}
HASH_FIELDS = ("image_sha256", "vars_sha256", "binary_hash", "config_sha256", "firmware_sha256")
COMMON_FIELDS = (
    "harness_commit", "image_sha256", "vars_sha256", "config_sha256",
    "firmware_sha256", "host_model", "macos_version", "power_source_start",
    "power_source_end", "smp_cpus", "ram_mib", "binary_profile",
    "binary_features", "rust_toolchain", "known_confounders",
)
AA_FIXTURE_ID = "0" * 32
AB_FIXTURE_ID = "1" * 32


class EvidenceError(Exception):
    """An evidence failure with the attempts needed to audit it."""

    def __init__(self, errors: list[str], attempts: list[dict[str, Any]] | None = None):
        super().__init__("; ".join(errors))
        self.errors = errors
        self.attempts = attempts or []

    def document(self) -> dict[str, Any]:
        return {
            "error": "campaign evidence is incomplete or invalid",
            "errors": self.errors,
            "attempts": self.attempts,
            "exploratory": True,
            "claim_eligible": False,
        }


def _hex(value: Any, width: int) -> bool:
    return isinstance(value, str) and len(value) == width and all(c in "0123456789abcdef" for c in value)


def _integer(value: str, name: str) -> int:
    if not value.isascii() or not value.isdigit() or str(int(value)) != value:
        raise ValueError(f"{name} must be a canonical positive integer")
    result = int(value)
    if result < 1:
        raise ValueError(f"{name} must be positive")
    return result


def _rows(path: Path) -> list[list[str]]:
    return [line.split("\t") for line in path.read_text().splitlines() if line]


def _mentions(path: Path, campaign_id: str) -> bool:
    try:
        return any(len(row) >= 2 and row[0] == "campaign_id" and row[1] == campaign_id for row in _rows(path))
    except (OSError, UnicodeError):
        return False


def _manifest(path: Path) -> tuple[dict[str, str], dict[str, tuple[str, str]]]:
    metadata: dict[str, str] = {}
    resources: dict[str, tuple[str, str]] = {}
    rows = _rows(path)
    for row in rows:
        key = row[0] if row else ""
        if key in resources or key in metadata:
            raise ValueError(f"duplicate manifest key {key!r}")
        if key in RESOURCE_KEYS and len(row) == 3:
            if not Path(row[1]).is_absolute() or not _hex(row[2], 64):
                raise ValueError(f"invalid resource row {key!r}")
            resources[key] = (row[1], row[2])
        elif key in META_KEYS and len(row) == 2 and row[1]:
            metadata[key] = row[1]
        else:
            raise ValueError(f"invalid manifest row for {key!r}")
    if len(rows) != 12 or set(resources) != RESOURCE_KEYS or set(metadata) != META_KEYS:
        raise ValueError("manifest must contain exactly 12 unique resource/metadata rows")
    if metadata["campaign_mode"] not in ("AA", "AB"):
        raise ValueError("campaign_mode must be AA or AB")
    if metadata["campaign_role"] not in ("baseline", "candidate"):
        raise ValueError("campaign_role must be baseline or candidate")
    if not _hex(metadata["binary_source_commit"], 40):
        raise ValueError("binary_source_commit must be 40 lowercase hex characters")
    if not _hex(metadata["campaign_id"], 32):
        raise ValueError("campaign_id must be 32 lowercase hex characters")
    if metadata["binary_profile"] != "release":
        raise ValueError("binary_profile must be release")
    if not re.fullmatch(r"[a-z0-9,+_-]+", metadata["binary_features"]):
        raise ValueError("binary_features is not canonical")
    if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", metadata["rust_toolchain"]):
        raise ValueError("rust_toolchain must be an exact stable version")
    ordinal = _integer(metadata["campaign_ordinal"], "campaign_ordinal")
    expected = _integer(metadata["campaign_expected_runs"], "campaign_expected_runs")
    if expected < 6 or expected % 2 or ordinal > expected:
        raise ValueError("campaign expected runs must be even and >=6, with ordinal in range")
    return metadata, resources


def _job_env(path: Path) -> dict[str, str]:
    result: dict[str, str] = {}
    for line in path.read_text().splitlines():
        key, separator, value = line.partition("=")
        if not separator or not key or key in result:
            raise ValueError("malformed or duplicate job.env entry")
        result[key] = value
    return result


def _timestamp(value: Any, name: str) -> datetime:
    if not isinstance(value, str):
        raise ValueError(f"{name} must be an ISO-8601 timestamp")
    parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
    if parsed.tzinfo is None:
        raise ValueError(f"{name} must include a timezone")
    return parsed


def _validate_receipt(job: dict[str, Any], metadata: dict[str, str], resources: dict[str, tuple[str, str]]) -> None:
    receipt = job["receipt"]
    expected = {
        "campaign_id": metadata["campaign_id"], "campaign_mode": metadata["campaign_mode"],
        "campaign_role": metadata["campaign_role"],
        "campaign_ordinal": int(metadata["campaign_ordinal"]),
        "campaign_expected_runs": int(metadata["campaign_expected_runs"]),
        "binary_source_commit": metadata["binary_source_commit"],
        "binary_profile": metadata["binary_profile"], "binary_features": metadata["binary_features"],
        "rust_toolchain": metadata["rust_toolchain"], "image_sha256": resources["image"][1],
        "vars_sha256": resources["vars"][1], "binary_hash": resources["binary"][1],
    }
    for field, value in expected.items():
        actual = receipt.get(field)
        if actual != value or (isinstance(value, int) and type(actual) is not int):
            raise ValueError(f"receipt {field} does not match its sealed manifest")
    fixed = {
        "schema_version": 2, "tier": "t15-hvf-boot-performance", "gate_id": "hvf-boot-performance-diagnostic",
        "sample_count": 1, "run_count": 1, "required_run_count": 1,
        "passes": 1, "failures": 0, "outcome": "completed",
    }
    for field, value in fixed.items():
        actual = receipt.get(field)
        if actual != value or (isinstance(value, int) and type(actual) is not int):
            raise ValueError(f"receipt has invalid {field}")
    if receipt.get("pass") is not True or receipt.get("valid") is not True:
        raise ValueError("receipt is not passing and valid")
    if receipt.get("job_id") != job["job_id"]:
        raise ValueError("receipt job_id does not match queue directory")
    if job["job_env"].get("job_id") != job["job_id"] or job["job_env"].get("tier") != fixed["tier"]:
        raise ValueError("job.env identity does not match queue directory and tier")
    if job["result_env"].get("result") != "pass" or job["result_env"].get("exit_code") != "0":
        raise ValueError("worker result is not a successful zero-exit attempt")
    harness = receipt.get("harness_commit")
    if not _hex(harness, 40) or harness != job["job_env"].get("commit"):
        raise ValueError("receipt harness_commit does not match sealed queue commit")
    if receipt.get("commit") != harness:
        raise ValueError("receipt commit does not match harness_commit")
    if receipt.get("tested_commit") != receipt.get("binary_source_commit"):
        raise ValueError("receipt tested_commit does not match binary_source_commit")
    manifest_hash = hashlib.sha256(job["manifest_path"].read_bytes()).hexdigest()
    if receipt.get("input_manifest_sha256") != manifest_hash or job["job_env"].get("input_manifest_sha256") != manifest_hash:
        raise ValueError("receipt and queue do not seal the input manifest")
    if job["job_env"].get("sealed_binary_sha256") != resources["binary"][1]:
        raise ValueError("queue binary identity does not match the sealed manifest")
    for field in HASH_FIELDS:
        if not _hex(receipt.get(field), 64):
            raise ValueError(f"receipt {field} must be 64 lowercase hex characters")
    elapsed = receipt.get("desktop_elapsed_ms")
    if isinstance(elapsed, bool) or not isinstance(elapsed, (int, float)) or not math.isfinite(elapsed) or elapsed <= 0:
        raise ValueError("desktop_elapsed_ms must be finite and positive")
    started = _timestamp(receipt.get("started_at"), "started_at")
    finished = _timestamp(receipt.get("finished_at"), "finished_at")
    if finished <= started:
        raise ValueError("finished_at must be after started_at")
    for field in ("host_model", "macos_version", "power_source_start", "power_source_end"):
        if not isinstance(receipt.get(field), str) or receipt[field].strip().lower() in ("", "unknown"):
            raise ValueError(f"receipt has missing or unknown {field}")
    if receipt["power_source_start"] != receipt["power_source_end"]:
        raise ValueError("power source changed during the sample")
    for field in ("smp_cpus", "ram_mib"):
        if type(receipt.get(field)) is not int or receipt[field] < 1:
            raise ValueError(f"receipt {field} must be a positive integer")
    if not isinstance(receipt.get("known_confounders"), list) or "full clone integrity hash immediately precedes boot (warm cache)" not in receipt["known_confounders"] or not all(
        isinstance(item, str) and item for item in receipt["known_confounders"]
    ):
        raise ValueError("known_confounders must be a string list")
    job["started"] = started
    job["finished"] = finished


def _attempt(job: dict[str, Any]) -> dict[str, Any]:
    metadata = job.get("metadata", {})
    resources = job.get("resources", {})
    receipt = job.get("receipt", {})
    fallback_hash = {
        "image_sha256": resources.get("image", (None, None))[1],
        "vars_sha256": resources.get("vars", (None, None))[1],
        "binary_hash": resources.get("binary", (None, None))[1],
    }
    identity_fields = (
        "harness_commit", "binary_source_commit", "binary_profile", "binary_features",
        "rust_toolchain", "input_manifest_sha256", *HASH_FIELDS, "host_model", "macos_version",
        "power_source_start", "power_source_end", "smp_cpus", "ram_mib",
        "known_confounders",
    )
    identity = {
        field: receipt.get(field, fallback_hash.get(field, metadata.get(field)))
        for field in identity_fields
    }
    return {
        "job_id": job["job_id"], "state": job["state"],
        "campaign_id": metadata.get("campaign_id"), "mode": metadata.get("campaign_mode"),
        "role": metadata.get("campaign_role"),
        "ordinal": int(metadata["campaign_ordinal"]) if metadata.get("campaign_ordinal", "").isdigit() else None,
        "started_at": receipt.get("started_at"), "finished_at": receipt.get("finished_at"),
        "pass": receipt.get("pass"), "valid": receipt.get("valid"), "identity": identity,
    }


def _discover(root: Path, campaign_id: str) -> list[dict[str, Any]]:
    jobs: list[dict[str, Any]] = []
    for state in ("queued", "running", "done"):
        state_dir = root / state
        if not state_dir.is_dir():
            continue
        for manifest_path in sorted(state_dir.glob("*/input-manifest.tsv")):
            if _mentions(manifest_path, campaign_id):
                jobs.append({
                    "job_id": manifest_path.parent.name, "state": state,
                    "manifest_path": manifest_path,
                })
    return jobs


def load_campaign(root: Path, campaign_id: str, required_mode: str) -> dict[str, Any]:
    jobs = _discover(root, campaign_id)
    errors: list[str] = []
    if not jobs:
        raise EvidenceError([f"campaign {campaign_id!r} has no registered attempts"])
    for job in jobs:
        try:
            metadata, resources = _manifest(job["manifest_path"])
            job["metadata"], job["resources"] = metadata, resources
            if metadata["campaign_id"] != campaign_id or metadata["campaign_mode"] != required_mode:
                raise ValueError(f"campaign must have id {campaign_id!r} and mode {required_mode}")
            if job["state"] != "done":
                raise ValueError(f"attempt remains in {job['state']} state")
            job["job_env"] = _job_env(job["manifest_path"].parent / "job.env")
            job["result_env"] = _job_env(job["manifest_path"].parent / "result.env")
            receipt_path = job["manifest_path"].parent / "receipt.public.json"
            if not receipt_path.exists():
                receipt_path = job["manifest_path"].parent / "receipt.json"
            job["receipt_path"] = receipt_path
            job["receipt"] = json.loads(receipt_path.read_text())
            _validate_receipt(job, metadata, resources)
        except (OSError, UnicodeError, json.JSONDecodeError, ValueError) as exc:
            errors.append(f"{job['job_id']}: {exc}")
    attempts = [_attempt(job) for job in jobs]
    valid_jobs = [job for job in jobs if "started" in job]
    ids = [job["job_id"] for job in jobs]
    if len(ids) != len(set(ids)):
        errors.append("campaign contains duplicate job_ids")
    if valid_jobs:
        expected_values = {int(job["metadata"]["campaign_expected_runs"]) for job in valid_jobs}
        ordinals = [int(job["metadata"]["campaign_ordinal"]) for job in valid_jobs]
        if len(expected_values) != 1:
            errors.append("campaign_expected_runs is inconsistent")
        else:
            expected = next(iter(expected_values))
            if sorted(ordinals) != list(range(1, expected + 1)):
                errors.append(f"campaign ordinals do not exactly cover 1..{expected}")
        ordered = sorted(valid_jobs, key=lambda job: int(job["metadata"]["campaign_ordinal"]))
        for index, job in enumerate(ordered):
            ordinal = int(job["metadata"]["campaign_ordinal"])
            required_role = "baseline" if ordinal % 2 == 1 else "candidate"
            if job["metadata"]["campaign_role"] != required_role:
                errors.append(f"ordinal {ordinal} must have role {required_role}")
            if index and (job["started"] < ordered[index - 1]["started"] or job["started"] < ordered[index - 1]["finished"]):
                errors.append(f"ordinal {index + 1} is out of order or overlaps its predecessor")
    else:
        ordered = []
    if errors:
        raise EvidenceError(errors, attempts)
    reference = ordered[0]["receipt"]
    for job in ordered[1:]:
        for field in COMMON_FIELDS:
            if job["receipt"].get(field) != reference.get(field):
                errors.append(f"campaign has mismatched common identity {field}")
    baseline = [job for job in ordered if job["metadata"]["campaign_role"] == "baseline"]
    candidate = [job for job in ordered if job["metadata"]["campaign_role"] == "candidate"]
    for side, name in ((baseline, "baseline"), (candidate, "candidate")):
        if len({(job["receipt"]["binary_hash"], job["receipt"]["binary_source_commit"]) for job in side}) != 1:
            errors.append(f"{name} does not use one sealed binary/source identity")
    left = baseline[0]["receipt"]
    right = candidate[0]["receipt"]
    equal_binary = (left["binary_hash"], left["binary_source_commit"]) == (right["binary_hash"], right["binary_source_commit"])
    if (required_mode == "AA" and not equal_binary) or (required_mode == "AB" and equal_binary):
        errors.append(f"{required_mode} campaign has invalid binary/source relationship")
    if required_mode == "AB" and (
        left["binary_hash"] == right["binary_hash"] or left["binary_source_commit"] == right["binary_source_commit"]
    ):
        errors.append("AB requires distinct binary hashes and source commits")
    if errors:
        raise EvidenceError(errors, attempts)
    return {"id": campaign_id, "mode": required_mode, "jobs": ordered, "attempts": attempts}


def _percentile(values: list[float], quantile: float) -> float:
    ordered = sorted(values)
    return ordered[max(0, math.ceil(quantile * len(ordered)) - 1)]


def _paired_interval(before: list[float], after: list[float]) -> tuple[float, float, float]:
    deltas = [100.0 * (candidate - baseline) / baseline for baseline, candidate in zip(before, after)]
    rng = random.Random(0xB12D6E)
    bootstrap = [statistics.median(rng.choices(deltas, k=len(deltas))) for _ in range(10_000)]
    bootstrap.sort()
    return statistics.median(deltas), bootstrap[249], bootstrap[9749]


def _side(jobs: list[dict[str, Any]]) -> dict[str, Any]:
    receipts = [job["receipt"] for job in jobs]
    values = [float(receipt["desktop_elapsed_ms"]) for receipt in receipts]
    return {
        "binary_sha256": receipts[0]["binary_hash"],
        "binary_source_commit": receipts[0]["binary_source_commit"],
        "samples_ms": values, "p50_ms": statistics.median(values),
        "p95_ms": _percentile(values, 0.95),
    }


def _summary(campaign: dict[str, Any]) -> dict[str, Any]:
    jobs = campaign["jobs"]
    baseline_jobs = jobs[::2]
    candidate_jobs = jobs[1::2]
    before = [float(job["receipt"]["desktop_elapsed_ms"]) for job in baseline_jobs]
    after = [float(job["receipt"]["desktop_elapsed_ms"]) for job in candidate_jobs]
    estimate, lower, upper = _paired_interval(before, after)
    reference = jobs[0]["receipt"]
    return {
        "campaign_id": campaign["id"], "mode": campaign["mode"],
        "expected_runs": len(jobs), "pairs": len(baseline_jobs),
        "identity": {field: reference[field] for field in COMMON_FIELDS},
        "attempts": campaign["attempts"],
        "baseline": _side(baseline_jobs), "candidate": _side(candidate_jobs),
        "paired_median_delta_ms": statistics.median([a - b for b, a in zip(before, after)]),
        "paired_median_delta_percent": estimate,
        "paired_median_delta_percent_ci95": [lower, upper],
    }


def analyze(root: Path, aa_id: str, ab_id: str | None = None) -> dict[str, Any]:
    aa = load_campaign(root, aa_id, "AA")
    aa_summary = _summary(aa)
    aa_interval = aa_summary["paired_median_delta_percent_ci95"]
    result: dict[str, Any] = {
        "exploratory": True, "claim_eligible": False, "aa": aa_summary,
        "aa_noise_bound_percent": max(abs(aa_interval[0]), abs(aa_interval[1])),
    }
    if not ab_id:
        return result
    ab = load_campaign(root, ab_id, "AB")
    combined_ids = [job["job_id"] for job in aa["jobs"] + ab["jobs"]]
    if len(combined_ids) != len(set(combined_ids)):
        raise EvidenceError(["AA and AB campaigns reuse a job_id"], aa["attempts"] + ab["attempts"])
    aa_reference, ab_reference = aa["jobs"][0]["receipt"], ab["jobs"][0]["receipt"]
    errors = [f"AA and AB have mismatched common identity {field}" for field in COMMON_FIELDS if aa_reference[field] != ab_reference[field]]
    if (aa_reference["binary_hash"], aa_reference["binary_source_commit"]) != (
        ab_reference["binary_hash"], ab_reference["binary_source_commit"]
    ):
        errors.append("AB baseline does not match the AA baseline binary/source")
    if errors:
        raise EvidenceError(errors, aa["attempts"] + ab["attempts"])
    ab_summary = _summary(ab)
    upper = ab_summary["paired_median_delta_percent_ci95"][1]
    result["ab"] = ab_summary
    result["ab_exceeds_aa_noise"] = upper < -result["aa_noise_bound_percent"]
    return result


def _fixture(root: Path) -> None:
    def campaign(identifier: str, mode: str, source: str, other_source: str, binary: str, other_binary: str, day: int) -> None:
        for offset in range(6):
            ordinal = offset + 1
            role = "baseline" if offset % 2 == 0 else "candidate"
            prefix = "noise" if mode == "AA" else "change"
            job_id = f"{prefix}-{ordinal}"
            directory = root / "done" / job_id
            directory.mkdir(parents=True)
            selected_source = source if role == "baseline" else other_source
            selected_binary = binary if role == "baseline" else other_binary
            metadata = {
                "binary_source_commit": selected_source, "binary_profile": "release",
                "binary_features": "venus", "rust_toolchain": "1.97.0",
                "campaign_id": identifier, "campaign_mode": mode, "campaign_role": role,
                "campaign_ordinal": str(ordinal), "campaign_expected_runs": "6",
            }
            rows = [
                f"image\t/sealed/image.raw\t{'a' * 64}", f"vars\t/sealed/vars.fd\t{'b' * 64}",
                f"binary\t/sealed/probe\t{selected_binary}",
                *[f"{key}\t{value}" for key, value in metadata.items()],
            ]
            manifest_path = directory / "input-manifest.tsv"
            manifest_path.write_text("\n".join(rows) + "\n")
            manifest_hash = hashlib.sha256(manifest_path.read_bytes()).hexdigest()
            (directory / "job.env").write_text(
                f"job_id={job_id}\ntier=t15-hvf-boot-performance\ncommit={'1' * 40}\n"
                f"input_manifest_sha256={manifest_hash}\nsealed_binary_sha256={selected_binary}\n"
            )
            (directory / "result.env").write_text("result=pass\nexit_code=0\n")
            started = datetime(2026, 1, day, tzinfo=timezone.utc) + timedelta(minutes=offset * 2)
            finished = started + timedelta(minutes=1)
            receipt = {
                **metadata, "campaign_ordinal": ordinal, "campaign_expected_runs": 6,
                "schema_version": 2, "tier": "t15-hvf-boot-performance", "gate_id": "hvf-boot-performance-diagnostic",
                "job_id": job_id, "harness_commit": "1" * 40, "commit": "1" * 40,
                "tested_commit": selected_source, "image_sha256": "a" * 64,
                "vars_sha256": "b" * 64, "binary_hash": selected_binary,
                "input_manifest_sha256": manifest_hash,
                "config_sha256": "c" * 64, "firmware_sha256": "f" * 64,
                "host_model": "MacTest", "macos_version": "26.0",
                "power_source_start": "AC Power", "power_source_end": "AC Power",
                "smp_cpus": 4, "ram_mib": 6144, "known_confounders": ["full clone integrity hash immediately precedes boot (warm cache)"],
                "desktop_elapsed_ms": float(100 - (10 if mode == "AB" and role == "candidate" else 0)),
                "sample_count": 1, "run_count": 1, "required_run_count": 1,
                "passes": 1, "failures": 0, "outcome": "completed", "pass": True, "valid": True,
                "started_at": started.isoformat().replace("+00:00", "Z"),
                "finished_at": finished.isoformat().replace("+00:00", "Z"),
            }
            (directory / "receipt.public.json").write_text(json.dumps(receipt))
    campaign(AA_FIXTURE_ID, "AA", "2" * 40, "2" * 40, "d" * 64, "d" * 64, 1)
    campaign(AB_FIXTURE_ID, "AB", "2" * 40, "3" * 40, "d" * 64, "e" * 64, 2)


def _expect_rejection(mutate: Callable[[Path], None], needle: str) -> None:
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        _fixture(root)
        mutate(root)
        try:
            analyze(root, AA_FIXTURE_ID, AB_FIXTURE_ID)
        except EvidenceError as exc:
            assert needle in " ".join(exc.errors), exc.document()
        else:
            raise AssertionError(f"expected rejection containing {needle!r}")


def _edit_receipt(root: Path, job: str, edit: Callable[[dict[str, Any]], None]) -> None:
    path = root / "done" / job / "receipt.public.json"
    data = json.loads(path.read_text())
    edit(data)
    path.write_text(json.dumps(data))


def self_test() -> None:
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        _fixture(root)
        result = analyze(root, AA_FIXTURE_ID, AB_FIXTURE_ID)
        assert result["exploratory"] and not result["claim_eligible"]
        assert result["ab_exceeds_aa_noise"] and result["ab"]["paired_median_delta_percent"] == -10.0
    def duplicate(root: Path) -> None:
        shutil.copytree(root / "done" / "noise-1", root / "queued" / "noise-1")
    _expect_rejection(duplicate, "duplicate job_ids")
    _expect_rejection(lambda root: _edit_receipt(root, "noise-1", lambda data: data.pop("schema_version")), "schema_version")
    _expect_rejection(lambda root: _edit_receipt(root, "noise-1", lambda data: data.pop("firmware_sha256")), "firmware_sha256")
    for value in (float("nan"), 0.0):
        _expect_rejection(lambda root, value=value: _edit_receipt(root, "noise-1", lambda data: data.__setitem__("desktop_elapsed_ms", value)), "finite and positive")
    def harness(root: Path) -> None:
        def change(data: dict[str, Any]) -> None:
            for field in ("harness_commit", "commit"):
                data[field] = "9" * 40
        _edit_receipt(root, "noise-2", change)
        path = root / "done" / "noise-2" / "job.env"
        path.write_text(path.read_text().replace("1" * 40, "9" * 40))
    _expect_rejection(harness, "mismatched common identity harness_commit")
    def overlap(root: Path) -> None:
        first = json.loads((root / "done" / "noise-1" / "receipt.public.json").read_text())
        overlapping = _timestamp(first["finished_at"], "finished_at") - timedelta(seconds=30)
        value = overlapping.isoformat().replace("+00:00", "Z")
        _edit_receipt(root, "noise-2", lambda data: data.__setitem__("started_at", value))
    _expect_rejection(overlap, "out of order or overlaps")
    def out_of_order(root: Path) -> None:
        value = datetime(2025, 12, 31, 23, 59, tzinfo=timezone.utc).isoformat().replace("+00:00", "Z")
        _edit_receipt(root, "noise-2", lambda data: data.__setitem__("started_at", value))
    _expect_rejection(out_of_order, "out of order or overlaps")
    _expect_rejection(lambda root: shutil.rmtree(root / "done" / "noise-6"), "do not exactly cover")
    _expect_rejection(lambda root: (root / "done" / "noise-3" / "result.env").write_text("result=fail\nexit_code=1\n"), "worker result")
    _expect_rejection(lambda root: _edit_receipt(root, "noise-3", lambda data: data.__setitem__("pass", False)), "not passing and valid")
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        _fixture(root)
        try:
            analyze(root, "f" * 32, AB_FIXTURE_ID)
        except EvidenceError as exc:
            assert "no registered attempts" in " ".join(exc.errors)
        else:
            raise AssertionError("missing A/A campaign was accepted")
    print("HVF boot performance campaign reporter self-test: PASS")
