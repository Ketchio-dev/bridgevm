#!/usr/bin/env python3
"""Fail-closed contract for the preregistered 4-vCPU versus 6-vCPU campaign."""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import math
import random
import re
import statistics
import tempfile
from pathlib import Path

from hvf_smp_scaling import ContractError, _timestamp, sha256_file


RESOURCE_KEYS = ("image", "vars", "binary", "renderer")
METADATA_KEYS = (
    "binary_source_commit", "binary_profile", "binary_features", "rust_toolchain",
    "workload_profile", "smp_cpus", "pairs", "aa_noise_upper_percent",
)
WORKLOAD = "shipping-core-3d-off-smp-confirmation-v1"
SMP = (4, 6)
PAIRS = 12
EXPECTED_RUNS = PAIRS * 2
AA_NOISE_UPPER = 16.6973
BOOTSTRAP_SEED = 0xB12D6E


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
    expected = {
        "workload_profile": WORKLOAD, "smp_cpus": "4,6", "pairs": str(PAIRS),
        "aa_noise_upper_percent": f"{AA_NOISE_UPPER:.4f}",
    }
    for key, value in expected.items():
        if metadata[key] != value:
            raise ContractError(f"{key} must be exactly {value}")
    return {"resources": resources, "metadata": metadata}


def _smp(config: str) -> int:
    match = re.search(r"(?:^|,)smp=([0-9]+)(?:,|$)", config)
    if not match:
        raise ContractError("matrix row lacks an SMP identity")
    return int(match.group(1))


def _pair_order(pair: int) -> tuple[int, int]:
    return SMP if pair % 2 else tuple(reversed(SMP))


def _percentile(values: list[float], quantile: float) -> float:
    ordered = sorted(values)
    return ordered[max(0, math.ceil(quantile * len(ordered)) - 1)]


def _interval(deltas: list[float]) -> tuple[float, float, float]:
    rng = random.Random(BOOTSTRAP_SEED)
    bootstrap = sorted(statistics.median(rng.choices(deltas, k=len(deltas))) for _ in range(10_000))
    return statistics.median(deltas), bootstrap[249], bootstrap[9749]


def _hash_set(paths: list[Path], root: Path) -> str:
    digest = hashlib.sha256()
    for path in paths:
        relative = path.relative_to(root).as_posix().encode()
        digest.update(len(relative).to_bytes(4, "big"))
        digest.update(relative)
        digest.update(bytes.fromhex(sha256_file(path)))
    return digest.hexdigest()


def analyze(matrix_root: Path, manifest: dict[str, object]) -> dict[str, object]:
    expected_dirs = [matrix_root / f"pair-{pair:02d}" for pair in range(1, PAIRS + 1)]
    observed_dirs = sorted(path for path in matrix_root.glob("pair-*") if path.is_dir())
    if observed_dirs != expected_dirs:
        raise ContractError("matrix does not contain exactly pair-01 through pair-12")
    resources = manifest["resources"]
    samples: dict[int, list[float]] = {4: [], 6: []}
    orders: list[str] = []
    reports: list[Path] = []
    logs: list[Path] = []
    for pair, pair_dir in enumerate(expected_dirs, 1):
        expected_order = _pair_order(pair)
        report = pair_dir / "boot-timer-report.tsv"
        rows = list(csv.reader(report.read_text(encoding="utf-8").splitlines(), delimiter="\t"))
        run_rows = [row for row in rows if row and row[0] == "run"]
        if len(run_rows) != 2:
            raise ContractError(f"pair {pair} must contain exactly two run rows")
        observed_order: list[int] = []
        for ordinal, row in enumerate(run_rows, 1):
            if len(row) != 14:
                raise ContractError(f"pair {pair} row {ordinal} has an invalid field count")
            smp = _smp(row[1])
            if smp not in SMP or row[9] != str(smp):
                raise ContractError(f"pair {pair} row {ordinal} has an invalid SMP identity")
            if row[5] != "true" or row[11] != "0" or row[12] != "true" or row[13]:
                raise ContractError(f"pair {pair} row {ordinal} is not a valid READY plus clean-shutdown sample")
            try:
                desktop = float(row[4])
            except ValueError as exc:
                raise ContractError(f"pair {pair} row {ordinal} has a non-numeric desktop time") from exc
            if desktop <= 0:
                raise ContractError(f"pair {pair} row {ordinal} has a non-positive desktop time")
            integrity = pair_dir / f"smp-{smp}" / "run-1" / "media-integrity.txt"
            integrity_rows = dict(line.split("=", 1) for line in integrity.read_text().splitlines())
            if integrity_rows != {
                "target_sha256": resources["image"]["sha256"],
                "vars_sha256": resources["vars"]["sha256"],
            }:
                raise ContractError(f"pair {pair} SMP {smp} clone identity mismatch")
            log = pair_dir / f"smp-{smp}" / "run-1" / "run.log"
            if not log.is_file():
                raise ContractError(f"pair {pair} SMP {smp} run log is missing")
            observed_order.append(smp)
            samples[smp].append(desktop)
            logs.append(log)
        if tuple(observed_order) != expected_order:
            raise ContractError(f"pair {pair} is not in the preregistered order")
        orders.append(",".join(map(str, observed_order)))
        reports.append(report)
    deltas = [100.0 * (six - four) / four for four, six in zip(samples[4], samples[6])]
    estimate, lower, upper = _interval(deltas)
    four_p95, six_p95 = _percentile(samples[4], 0.95), _percentile(samples[6], 0.95)
    conditions = {
        "all_runs_valid": True,
        "paired_median_below_zero": estimate < 0,
        "bootstrap_ci_entirely_below_zero": upper < 0,
        "improvement_exceeds_aa_noise": -estimate > AA_NOISE_UPPER,
        "six_vcpu_p95_not_worse": six_p95 <= four_p95,
    }
    return {
        "pair_count": PAIRS, "sample_count": EXPECTED_RUNS,
        "pair_execution_order": orders,
        "four_vcpu_samples_ms": samples[4], "six_vcpu_samples_ms": samples[6],
        "four_vcpu_p50_ms": statistics.median(samples[4]),
        "six_vcpu_p50_ms": statistics.median(samples[6]),
        "four_vcpu_p95_ms": four_p95, "six_vcpu_p95_ms": six_p95,
        "paired_delta_percent": deltas,
        "paired_median_delta_percent": estimate,
        "paired_median_delta_percent_ci95": [lower, upper],
        "aa_noise_upper_percent": AA_NOISE_UPPER,
        "decision_conditions": conditions,
        "product_default_candidate_worth_implementing": all(conditions.values()),
        "matrix_report_set_sha256": _hash_set(reports, matrix_root),
        "run_log_set_sha256": _hash_set(logs, matrix_root),
    }


def write_receipt(args: argparse.Namespace) -> dict[str, object]:
    sealed = load_manifest(args.manifest)
    metadata, resources = sealed["metadata"], sealed["resources"]
    started, finished = _timestamp(args.started_at, "started_at"), _timestamp(args.finished_at, "finished_at")
    if finished <= started:
        raise ContractError("finished_at must be after started_at")
    reason, summary = args.invalid_reason, {}
    if not reason:
        try:
            summary = analyze(args.matrix_root, sealed)
        except (OSError, UnicodeError, ContractError) as exc:
            reason = str(exc)
    if args.power_start != args.power_end:
        reason = reason or "power source changed during the campaign"
    passed = not reason
    config = (
        f"{WORKLOAD};release;skip-build;daily;pairs=12;counterbalanced=alternating-ab-ba;ram=6144;"
        f"virtio-net;xhci;hda-coreaudio;virtio-gpu-3d=off;balanced;agent-ready;"
        f"shutdown;watchdog=120000;firmware={args.firmware_sha256};renderer={resources['renderer']['sha256']}"
    )
    receipt: dict[str, object] = {
        "schema_version": 1, "tier": "d8-hvf-smp-confirmation",
        "gate_id": "hvf-smp-confirmation-diagnostic", "job_id": args.job_id,
        "commit": args.commit, "tested_commit": metadata["binary_source_commit"],
        "harness_commit": args.commit, "binary_source_commit": metadata["binary_source_commit"],
        "binary_profile": metadata["binary_profile"], "binary_features": metadata["binary_features"],
        "rust_toolchain": metadata["rust_toolchain"], "binary_hash": resources["binary"]["sha256"],
        "image_sha256": resources["image"]["sha256"], "vars_sha256": resources["vars"]["sha256"],
        "renderer_sha256": resources["renderer"]["sha256"], "firmware_sha256": args.firmware_sha256,
        "config_sha256": hashlib.sha256(config.encode()).hexdigest(),
        "input_manifest_sha256": sha256_file(args.manifest), "workload_profile": metadata["workload_profile"],
        "host_model": args.host_model, "macos_version": args.macos_version,
        "power_source_start": args.power_start, "power_source_end": args.power_end,
        "started_at": args.started_at, "finished_at": args.finished_at,
        "run_count": summary.get("sample_count", 0), "required_run_count": EXPECTED_RUNS,
        "passes": summary.get("sample_count", 0) if passed else 0, "failures": 0 if passed else 1,
        "outcome": "completed" if passed else "failed", "pass": passed, "valid": passed,
        "invalid_reason": reason, "claim_eligible": False, "criterion_pass": False,
        "capability_promotion": False, "three_d_injection": False,
        "known_confounders": [
            "full clone integrity hash immediately precedes boot (warm cache)",
            "boot harness omits product vTPM, clipboard/share, and long-lived app session",
        ], **summary,
    }
    verify_receipt(receipt, expected_commit=args.commit)
    return receipt


def verify_receipt(receipt: object, *, expected_commit: str | None = None) -> dict[str, object]:
    if not isinstance(receipt, dict):
        raise ContractError("receipt must be an object")
    required = {
        "schema_version", "tier", "gate_id", "job_id", "commit", "tested_commit", "harness_commit",
        "binary_source_commit", "binary_profile", "binary_features", "rust_toolchain", "binary_hash",
        "image_sha256", "vars_sha256", "renderer_sha256", "firmware_sha256", "config_sha256",
        "input_manifest_sha256", "workload_profile", "host_model", "macos_version", "power_source_start",
        "power_source_end", "started_at", "finished_at", "run_count", "required_run_count", "passes",
        "failures", "outcome", "pass", "valid", "invalid_reason", "claim_eligible", "criterion_pass",
        "capability_promotion", "three_d_injection", "known_confounders",
    }
    missing = required - receipt.keys()
    if missing:
        raise ContractError(f"receipt lacks required fields: {sorted(missing)}")
    if receipt["schema_version"] != 1 or receipt["tier"] != "d8-hvf-smp-confirmation" or receipt["gate_id"] != "hvf-smp-confirmation-diagnostic":
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
            raise ContractError(f"confirmation receipt must keep {field}=false")
    passed = receipt["pass"] is True and receipt["valid"] is True
    if passed:
        if (receipt["run_count"], receipt["required_run_count"], receipt["passes"], receipt["failures"]) != (24, 24, 24, 0):
            raise ContractError("passing receipt does not describe 24 successful runs")
        if receipt["outcome"] != "completed" or receipt["invalid_reason"] != "":
            raise ContractError("passing receipt outcome is inconsistent")
        expected = {
            "pair_count", "sample_count", "pair_execution_order", "four_vcpu_samples_ms",
            "six_vcpu_samples_ms", "four_vcpu_p50_ms", "six_vcpu_p50_ms", "four_vcpu_p95_ms",
            "six_vcpu_p95_ms", "paired_delta_percent", "paired_median_delta_percent",
            "paired_median_delta_percent_ci95", "aa_noise_upper_percent", "decision_conditions",
            "product_default_candidate_worth_implementing", "matrix_report_set_sha256", "run_log_set_sha256",
        }
        if not expected <= receipt.keys() or receipt["pair_count"] != PAIRS or receipt["sample_count"] != EXPECTED_RUNS:
            raise ContractError("passing receipt lacks the canonical confirmation summary")
        if receipt["aa_noise_upper_percent"] != AA_NOISE_UPPER:
            raise ContractError("receipt changed the preregistered A/A noise bound")
        for field in ("matrix_report_set_sha256", "run_log_set_sha256"):
            if not isinstance(receipt[field], str) or not re.fullmatch(r"[0-9a-f]{64}", receipt[field]):
                raise ContractError(f"passing receipt lacks {field}")
        conditions = receipt["decision_conditions"]
        if not isinstance(conditions, dict) or set(conditions) != {
            "all_runs_valid", "paired_median_below_zero", "bootstrap_ci_entirely_below_zero",
            "improvement_exceeds_aa_noise", "six_vcpu_p95_not_worse",
        } or not all(isinstance(value, bool) for value in conditions.values()):
            raise ContractError("receipt decision conditions are invalid")
        if receipt["product_default_candidate_worth_implementing"] is not all(conditions.values()):
            raise ContractError("receipt product-default decision is inconsistent")
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
            f"binary_source_commit\t{'a' * 40}", "binary_profile\trelease", "binary_features\tvenus",
            "rust_toolchain\t1.94.0", f"workload_profile\t{WORKLOAD}", "smp_cpus\t4,6",
            "pairs\t12", f"aa_noise_upper_percent\t{AA_NOISE_UPPER:.4f}",
        ]
        manifest_path = root / "manifest.tsv"
        manifest_path.write_text("\n".join(resources + metadata) + "\n")
        manifest = load_manifest(manifest_path)
        matrix = root / "matrix"
        for pair in range(1, PAIRS + 1):
            pair_dir = matrix / f"pair-{pair:02d}"
            lines = ["section\tconfig\tsource\tsummary_elapsed_ms\tdesktop_elapsed_ms\tdesktop_reached\tmilestones\ttotal_exits\ttotal_exits_per_sec\tvcpus\tgeneration\trun_status\tvalid\tinvalid_reason"]
            for ordinal, smp in enumerate(_pair_order(pair), 1):
                desktop = 30_000 if smp == 4 else 24_000
                lines.append(f"run\tprofile=release,smp={smp},gpu3d=0\t/source\t40000\t{desktop + pair}\ttrue\t2/8\t100\t10\t{smp}\t1\t0\ttrue\t")
                lane = pair_dir / f"smp-{smp}" / "run-1"
                lane.mkdir(parents=True)
                lane.joinpath("run.log").write_text(f"fixture {pair} {ordinal}\n")
                lane.joinpath("media-integrity.txt").write_text(
                    f"target_sha256={manifest['resources']['image']['sha256']}\n"
                    f"vars_sha256={manifest['resources']['vars']['sha256']}\n"
                )
            pair_dir.joinpath("boot-timer-report.tsv").write_text("\n".join(lines) + "\n")
        summary = analyze(matrix, manifest)
        assert summary["sample_count"] == 24 and summary["product_default_candidate_worth_implementing"] is True
        args = argparse.Namespace(
            manifest=manifest_path, matrix_root=matrix, output=root / "receipt.json", job_id="fixture",
            commit="b" * 40, started_at="2026-01-01T00:00:00Z", finished_at="2026-01-01T00:10:00Z",
            host_model="MacTest", macos_version="27.0", power_start="AC Power", power_end="AC Power",
            firmware_sha256="f" * 64, invalid_reason="",
        )
        receipt = write_receipt(args)
        assert receipt["pass"] is True and receipt["run_count"] == 24
        poisoned = dict(receipt, claim_eligible=True)
        try:
            verify_receipt(poisoned)
        except ContractError as exc:
            assert "claim_eligible" in str(exc)
        else:
            raise AssertionError("promotable confirmation receipt was accepted")
        wrong = matrix / "pair-02" / "boot-timer-report.tsv"
        wrong.write_text(wrong.read_text().replace("smp=6", "smp=4", 1))
        try:
            analyze(matrix, manifest)
        except ContractError as exc:
            assert "pair 2" in str(exc)
        else:
            raise AssertionError("wrong pair order was accepted")
    print("HVF SMP confirmation contract self-test: PASS")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    validate = sub.add_parser("validate-manifest")
    validate.add_argument("manifest", type=Path)
    validate.add_argument("--skip-resource-hashes", action="store_true")
    analyze_parser = sub.add_parser("analyze")
    analyze_parser.add_argument("matrix_root", type=Path)
    analyze_parser.add_argument("--manifest", type=Path, required=True)
    analyze_parser.add_argument("--output", type=Path)
    write = sub.add_parser("write-receipt")
    write.add_argument("--manifest", type=Path, required=True)
    write.add_argument("--matrix-root", type=Path, required=True)
    write.add_argument("--output", type=Path, required=True)
    write.add_argument("--job-id", required=True); write.add_argument("--commit", required=True)
    write.add_argument("--started-at", required=True); write.add_argument("--finished-at", required=True)
    write.add_argument("--host-model", required=True); write.add_argument("--macos-version", required=True)
    write.add_argument("--power-start", required=True); write.add_argument("--power-end", required=True)
    write.add_argument("--firmware-sha256", required=True); write.add_argument("--invalid-reason", default="")
    verify = sub.add_parser("verify-receipt")
    verify.add_argument("receipt", type=Path); verify.add_argument("--expected-commit")
    sub.add_parser("self-test")
    args = parser.parse_args()
    try:
        if args.command == "validate-manifest":
            result = load_manifest(args.manifest, verify_resources=not args.skip_resource_hashes)
        elif args.command == "analyze":
            result = analyze(args.matrix_root, load_manifest(args.manifest))
        elif args.command == "write-receipt":
            result = write_receipt(args)
        elif args.command == "verify-receipt":
            result = verify_receipt(json.loads(args.receipt.read_text()), expected_commit=args.expected_commit)
        else:
            self_test(); return 0
    except (OSError, UnicodeError, ContractError) as exc:
        print(json.dumps({"valid": False, "error": str(exc)}, sort_keys=True)); return 1
    rendered = json.dumps(result, indent=2, sort_keys=True) + "\n"
    if getattr(args, "output", None):
        args.output.write_text(rendered)
    else:
        print(rendered, end="")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
