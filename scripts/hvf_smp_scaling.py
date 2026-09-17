#!/usr/bin/env python3
"""Fail-closed contracts for the exploratory 4/6/8-vCPU boot matrix."""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import re
import statistics
import tempfile
from datetime import datetime
from pathlib import Path


RESOURCE_KEYS = ("image", "vars", "binary", "renderer")
METADATA_KEYS = (
    "binary_source_commit",
    "binary_profile",
    "binary_features",
    "rust_toolchain",
    "workload_profile",
    "smp_cpus",
    "runs_per_config",
)
EXPECTED_SMP = (4, 6, 8)
EXPECTED_RUNS = 3
WORKLOAD = "shipping-core-3d-off-smp-scaling-v1"


class ContractError(ValueError):
    """The sealed input or matrix output does not meet the diagnostic contract."""


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def load_manifest(path: Path, *, verify_resources: bool = True) -> dict[str, object]:
    rows = [line.split("\t") for line in path.read_text(encoding="utf-8").splitlines() if line]
    resources: dict[str, dict[str, str]] = {}
    metadata: dict[str, str] = {}
    for row in rows:
        key = row[0] if row else ""
        if key in resources or key in metadata:
            raise ContractError(f"duplicate manifest key {key!r}")
        if key in RESOURCE_KEYS and len(row) == 3:
            resource = Path(row[1])
            if not resource.is_absolute() or not re.fullmatch(r"[0-9a-f]{64}", row[2]):
                raise ContractError(f"invalid resource row {key!r}")
            if verify_resources:
                if not resource.is_file() or resource.is_symlink():
                    raise ContractError(f"resource {key!r} is missing or not a regular non-symlink file")
                if sha256_file(resource) != row[2]:
                    raise ContractError(f"resource {key!r} hash mismatch")
            resources[key] = {"path": str(resource), "sha256": row[2]}
        elif key in METADATA_KEYS and len(row) == 2 and row[1]:
            metadata[key] = row[1]
        else:
            raise ContractError(f"invalid manifest row {key!r}")
    if len(rows) != len(RESOURCE_KEYS) + len(METADATA_KEYS):
        raise ContractError("manifest has an unexpected row count")
    if tuple(resources) != RESOURCE_KEYS or tuple(metadata) != METADATA_KEYS:
        raise ContractError("manifest rows must use the canonical order")
    if not re.fullmatch(r"[0-9a-f]{40}", metadata["binary_source_commit"]):
        raise ContractError("binary_source_commit is not canonical")
    if metadata["binary_profile"] != "release":
        raise ContractError("binary_profile must be release")
    if not re.fullmatch(r"[a-z0-9,+_-]+", metadata["binary_features"]):
        raise ContractError("binary_features is not canonical")
    if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", metadata["rust_toolchain"]):
        raise ContractError("rust_toolchain must be an exact stable version")
    if metadata["workload_profile"] != WORKLOAD:
        raise ContractError("workload_profile is not the fixed 3D-off SMP diagnostic")
    if metadata["smp_cpus"] != ",".join(map(str, EXPECTED_SMP)):
        raise ContractError("smp_cpus must be exactly 4,6,8")
    if metadata["runs_per_config"] != str(EXPECTED_RUNS):
        raise ContractError("runs_per_config must be exactly 3")
    return {"resources": resources, "metadata": metadata}


def _smp_from_config(config: str) -> int:
    match = re.search(r"(?:^|,)smp=([0-9]+)(?:,|$)", config)
    if not match:
        raise ContractError("matrix row lacks an SMP identity")
    return int(match.group(1))


def analyze_report(path: Path) -> dict[str, object]:
    rows = list(csv.reader(path.read_text(encoding="utf-8").splitlines(), delimiter="\t"))
    run_rows = [row for row in rows if row and row[0] == "run"]
    expected_order = list(EXPECTED_SMP) * EXPECTED_RUNS
    if len(run_rows) != len(expected_order):
        raise ContractError(f"matrix must contain exactly {len(expected_order)} run rows")
    order: list[int] = []
    samples: list[float] = []
    by_smp: dict[int, list[float]] = {smp: [] for smp in EXPECTED_SMP}
    for ordinal, row in enumerate(run_rows, 1):
        if len(row) != 14:
            raise ContractError(f"run row {ordinal} has an invalid field count")
        smp = _smp_from_config(row[1])
        if smp not in by_smp:
            raise ContractError(f"run row {ordinal} uses an unexpected SMP value")
        if row[5] != "true" or row[11] != "0" or row[12] != "true" or row[13]:
            raise ContractError(f"run row {ordinal} is not a valid READY plus clean-shutdown sample")
        if row[9] != str(smp):
            raise ContractError(f"run row {ordinal} reports a mismatched vCPU count")
        try:
            desktop = float(row[4])
        except ValueError as exc:
            raise ContractError(f"run row {ordinal} has a non-numeric desktop time") from exc
        if desktop <= 0:
            raise ContractError(f"run row {ordinal} has a non-positive desktop time")
        order.append(smp)
        samples.append(desktop)
        by_smp[smp].append(desktop)
    if order != expected_order:
        raise ContractError("matrix runs are not in fixed round-robin 4,6,8 order")
    if any(len(by_smp[smp]) != EXPECTED_RUNS for smp in EXPECTED_SMP):
        raise ContractError("matrix does not contain three samples per SMP configuration")
    medians = [statistics.median(by_smp[smp]) for smp in EXPECTED_SMP]
    p95 = [max(by_smp[smp]) for smp in EXPECTED_SMP]
    baseline = medians[0]
    relative = [100.0 * (value - baseline) / baseline for value in medians]
    best_index = min(range(len(medians)), key=medians.__getitem__)
    return {
        "smp_configurations": list(EXPECTED_SMP),
        "runs_per_configuration": EXPECTED_RUNS,
        "sample_count": len(samples),
        "smp_execution_order": order,
        "desktop_samples_ms": samples,
        "desktop_medians_ms": medians,
        "desktop_p95_ms": p95,
        "median_delta_from_4_percent": relative,
        "best_smp_cpus": EXPECTED_SMP[best_index],
        "best_median_desktop_ms": medians[best_index],
    }


def _timestamp(value: str, name: str) -> datetime:
    try:
        parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError as exc:
        raise ContractError(f"{name} is not an ISO-8601 timestamp") from exc
    if parsed.tzinfo is None:
        raise ContractError(f"{name} lacks a timezone")
    return parsed


def _run_log_set_hash(matrix_root: Path) -> str:
    logs = sorted(matrix_root.glob("smp-*/run-*/run.log"))
    if len(logs) != len(EXPECTED_SMP) * EXPECTED_RUNS:
        raise ContractError("matrix does not retain exactly nine run logs")
    digest = hashlib.sha256()
    for log in logs:
        relative = log.relative_to(matrix_root).as_posix().encode()
        digest.update(len(relative).to_bytes(4, "big"))
        digest.update(relative)
        digest.update(bytes.fromhex(sha256_file(log)))
    return digest.hexdigest()


def write_receipt(args: argparse.Namespace) -> dict[str, object]:
    sealed = load_manifest(args.manifest)
    metadata = sealed["metadata"]
    resources = sealed["resources"]
    started = _timestamp(args.started_at, "started_at")
    finished = _timestamp(args.finished_at, "finished_at")
    if finished <= started:
        raise ContractError("finished_at must be after started_at")
    reason = args.invalid_reason
    summary: dict[str, object] = {}
    if not reason:
        try:
            summary = analyze_report(args.report)
            run_log_set_sha256 = _run_log_set_hash(args.matrix_root)
        except (OSError, UnicodeError, ContractError) as exc:
            reason = str(exc)
            run_log_set_sha256 = "absent"
    else:
        run_log_set_sha256 = "absent"
    if args.power_start != args.power_end:
        reason = reason or "power source changed during the matrix"
    passed = not reason
    config_material = (
        f"{WORKLOAD};release;skip-build;daily;smp=4,6,8;runs=3;ram=6144;"
        f"virtio-net;xhci;hda-coreaudio;virtio-gpu-3d=off;balanced;"
        f"agent-ready;shutdown;watchdog=120000;firmware={args.firmware_sha256};"
        f"renderer={resources['renderer']['sha256']}"
    )
    receipt: dict[str, object] = {
        "schema_version": 1,
        "tier": "d7-hvf-smp-scaling",
        "gate_id": "hvf-smp-scaling-diagnostic",
        "job_id": args.job_id,
        "commit": args.commit,
        "tested_commit": metadata["binary_source_commit"],
        "harness_commit": args.commit,
        "binary_source_commit": metadata["binary_source_commit"],
        "binary_profile": metadata["binary_profile"],
        "binary_features": metadata["binary_features"],
        "rust_toolchain": metadata["rust_toolchain"],
        "binary_hash": resources["binary"]["sha256"],
        "image_sha256": resources["image"]["sha256"],
        "vars_sha256": resources["vars"]["sha256"],
        "renderer_sha256": resources["renderer"]["sha256"],
        "firmware_sha256": args.firmware_sha256,
        "config_sha256": hashlib.sha256(config_material.encode()).hexdigest(),
        "input_manifest_sha256": sha256_file(args.manifest),
        "matrix_report_sha256": sha256_file(args.report) if args.report.is_file() else "absent",
        "run_log_set_sha256": run_log_set_sha256,
        "workload_profile": metadata["workload_profile"],
        "host_model": args.host_model,
        "macos_version": args.macos_version,
        "power_source_start": args.power_start,
        "power_source_end": args.power_end,
        "started_at": args.started_at,
        "finished_at": args.finished_at,
        "sample_count": summary.get("sample_count", 0),
        "run_count": summary.get("sample_count", 0),
        "required_run_count": len(EXPECTED_SMP) * EXPECTED_RUNS,
        "passes": summary.get("sample_count", 0) if passed else 0,
        "failures": 0 if passed else 1,
        "outcome": "completed" if passed else "failed",
        "pass": passed,
        "valid": passed,
        "invalid_reason": reason,
        "claim_eligible": False,
        "criterion_pass": False,
        "capability_promotion": False,
        "three_d_injection": False,
        "known_confounders": [
            "full clone integrity hash immediately precedes boot (warm cache)",
            "boot harness omits product vTPM, clipboard/share, and long-lived app session",
        ],
        **summary,
    }
    verify_receipt(receipt, expected_commit=args.commit)
    return receipt


def verify_receipt(receipt: object, *, expected_commit: str | None = None) -> dict[str, object]:
    if not isinstance(receipt, dict):
        raise ContractError("receipt must be an object")
    required = {
        "schema_version", "tier", "gate_id", "job_id", "commit", "tested_commit",
        "harness_commit", "binary_source_commit", "binary_profile", "binary_features",
        "rust_toolchain", "binary_hash", "image_sha256", "vars_sha256",
        "renderer_sha256", "firmware_sha256", "config_sha256", "input_manifest_sha256", "matrix_report_sha256",
        "run_log_set_sha256", "workload_profile", "host_model", "macos_version",
        "power_source_start", "power_source_end", "started_at", "finished_at",
        "sample_count", "run_count", "required_run_count", "passes", "failures",
        "outcome", "pass", "valid", "invalid_reason", "claim_eligible",
        "criterion_pass", "capability_promotion", "three_d_injection", "known_confounders",
    }
    missing = required - receipt.keys()
    if missing:
        raise ContractError(f"receipt lacks required fields: {sorted(missing)}")
    if receipt["schema_version"] != 1 or receipt["tier"] != "d7-hvf-smp-scaling" or receipt["gate_id"] != "hvf-smp-scaling-diagnostic":
        raise ContractError("receipt identity is invalid")
    for field in ("commit", "tested_commit", "harness_commit", "binary_source_commit"):
        if not isinstance(receipt[field], str) or not re.fullmatch(r"[0-9a-f]{40}", receipt[field]):
            raise ContractError(f"receipt {field} is not a commit hash")
    if expected_commit and receipt["commit"] != expected_commit:
        raise ContractError("receipt commit does not match the expected commit")
    for field in ("binary_hash", "image_sha256", "vars_sha256", "renderer_sha256", "firmware_sha256", "config_sha256", "input_manifest_sha256"):
        if not isinstance(receipt[field], str) or not re.fullmatch(r"[0-9a-f]{64}", receipt[field]):
            raise ContractError(f"receipt {field} is not a SHA-256")
    for field in ("claim_eligible", "criterion_pass", "capability_promotion", "three_d_injection"):
        if receipt[field] is not False:
            raise ContractError(f"diagnostic receipt must keep {field}=false")
    passed = receipt["pass"] is True and receipt["valid"] is True
    if passed:
        if (receipt["sample_count"], receipt["run_count"], receipt["required_run_count"], receipt["passes"], receipt["failures"]) != (9, 9, 9, 9, 0):
            raise ContractError("passing receipt does not describe nine successful runs")
        if receipt["outcome"] != "completed" or receipt["invalid_reason"] != "":
            raise ContractError("passing receipt outcome is inconsistent")
        for field in ("matrix_report_sha256", "run_log_set_sha256"):
            if not isinstance(receipt[field], str) or not re.fullmatch(r"[0-9a-f]{64}", receipt[field]):
                raise ContractError(f"passing receipt lacks {field}")
        expected_summary = {
            "smp_configurations", "runs_per_configuration", "smp_execution_order",
            "desktop_samples_ms", "desktop_medians_ms", "desktop_p95_ms",
            "median_delta_from_4_percent", "best_smp_cpus", "best_median_desktop_ms",
        }
        if not expected_summary <= receipt.keys():
            raise ContractError("passing receipt lacks the matrix summary")
        if receipt["smp_configurations"] != list(EXPECTED_SMP) or receipt["runs_per_configuration"] != EXPECTED_RUNS:
            raise ContractError("receipt SMP matrix is not canonical")
    elif receipt["outcome"] != "failed" or not receipt["invalid_reason"]:
        raise ContractError("failed receipt lacks an honest reason")
    return receipt


def self_test() -> None:
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        resources = []
        for key in RESOURCE_KEYS:
            resource = root / key
            resource.write_bytes(key.encode())
            resources.append(f"{key}\t{resource}\t{sha256_file(resource)}")
        metadata = [
            f"binary_source_commit\t{'a' * 40}",
            "binary_profile\trelease",
            "binary_features\tvenus",
            "rust_toolchain\t1.94.0",
            f"workload_profile\t{WORKLOAD}",
            "smp_cpus\t4,6,8",
            "runs_per_config\t3",
        ]
        manifest = root / "manifest.tsv"
        manifest.write_text("\n".join(resources + metadata) + "\n", encoding="utf-8")
        load_manifest(manifest)
        report = root / "report.tsv"
        lines = ["section\tconfig\tsource\tsummary_elapsed_ms\tdesktop_elapsed_ms\tdesktop_reached\tmilestones\ttotal_exits\ttotal_exits_per_sec\tvcpus\tgeneration\trun_status\tvalid\tinvalid_reason"]
        for run in range(3):
            for smp in EXPECTED_SMP:
                desktop = 30_000 - smp * 500 + run * 10
                lines.append(f"run\tprofile=release,smp={smp},gpu3d=0\t/source\t40000\t{desktop}\ttrue\t2/8\t100\t10\t{smp}\t1\t0\ttrue\t")
        report.write_text("\n".join(lines) + "\n", encoding="utf-8")
        result = analyze_report(report)
        assert result["sample_count"] == 9 and result["best_smp_cpus"] == 8
        matrix = root / "matrix"
        for run in range(1, 4):
            for smp in EXPECTED_SMP:
                log = matrix / f"smp-{smp}" / f"run-{run}" / "run.log"
                log.parent.mkdir(parents=True)
                log.write_text(f"fixture {smp} {run}\n")
        receipt_path = root / "receipt.json"
        args = argparse.Namespace(
            manifest=manifest, report=report, matrix_root=matrix, output=receipt_path,
            job_id="fixture", commit="b" * 40,
            started_at="2026-01-01T00:00:00Z", finished_at="2026-01-01T00:10:00Z",
            host_model="MacTest", macos_version="27.0",
            power_start="AC Power", power_end="AC Power",
            firmware_sha256="f" * 64, invalid_reason="",
        )
        receipt = write_receipt(args)
        assert receipt["pass"] is True and receipt["run_count"] == 9
        poisoned = dict(receipt, claim_eligible=True)
        try:
            verify_receipt(poisoned)
        except ContractError as exc:
            assert "claim_eligible" in str(exc)
        else:
            raise AssertionError("promotable diagnostic receipt was accepted")
        broken = report.read_text().replace("smp=6", "smp=8", 1)
        report.write_text(broken, encoding="utf-8")
        try:
            analyze_report(report)
        except ContractError as exc:
            assert "run row 2" in str(exc)
        else:
            raise AssertionError("out-of-order matrix was accepted")
    print("HVF SMP scaling contract self-test: PASS")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    validate = sub.add_parser("validate-manifest")
    validate.add_argument("manifest", type=Path)
    validate.add_argument("--skip-resource-hashes", action="store_true")
    analyze = sub.add_parser("analyze")
    analyze.add_argument("report", type=Path)
    analyze.add_argument("--output", type=Path)
    write = sub.add_parser("write-receipt")
    write.add_argument("--manifest", type=Path, required=True)
    write.add_argument("--report", type=Path, required=True)
    write.add_argument("--matrix-root", type=Path, required=True)
    write.add_argument("--output", type=Path, required=True)
    write.add_argument("--job-id", required=True)
    write.add_argument("--commit", required=True)
    write.add_argument("--started-at", required=True)
    write.add_argument("--finished-at", required=True)
    write.add_argument("--host-model", required=True)
    write.add_argument("--macos-version", required=True)
    write.add_argument("--power-start", required=True)
    write.add_argument("--power-end", required=True)
    write.add_argument("--firmware-sha256", required=True)
    write.add_argument("--invalid-reason", default="")
    verify = sub.add_parser("verify-receipt")
    verify.add_argument("receipt", type=Path)
    verify.add_argument("--expected-commit")
    sub.add_parser("self-test")
    args = parser.parse_args()
    try:
        if args.command == "validate-manifest":
            result = load_manifest(args.manifest, verify_resources=not args.skip_resource_hashes)
        elif args.command == "analyze":
            result = analyze_report(args.report)
        elif args.command == "write-receipt":
            result = write_receipt(args)
        elif args.command == "verify-receipt":
            result = verify_receipt(json.loads(args.receipt.read_text()), expected_commit=args.expected_commit)
        else:
            self_test()
            return 0
    except (OSError, UnicodeError, ContractError) as exc:
        print(json.dumps({"valid": False, "error": str(exc)}, sort_keys=True))
        return 1
    rendered = json.dumps(result, indent=2, sort_keys=True) + "\n"
    if getattr(args, "output", None):
        args.output.write_text(rendered, encoding="utf-8")
    else:
        print(rendered, end="")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
