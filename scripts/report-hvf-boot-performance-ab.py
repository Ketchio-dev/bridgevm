#!/usr/bin/env python3
"""Report paired sealed HVF boot samples with a deterministic bootstrap CI."""

from __future__ import annotations

import argparse
import json
import math
import random
import statistics
from pathlib import Path


def receipt(path: str) -> dict:
    candidate = Path(path)
    if candidate.is_dir():
        candidate = candidate / "receipt.public.json"
        if not candidate.exists():
            candidate = Path(path) / "receipt.json"
    data = json.loads(candidate.read_text())
    if not data.get("pass") or not data.get("valid"):
        raise ValueError(f"invalid performance sample: {candidate}")
    return data


def percentile(values: list[float], quantile: float) -> float:
    ordered = sorted(values)
    return ordered[max(0, math.ceil(quantile * len(ordered)) - 1)]


def paired_interval(baseline: list[float], candidate: list[float]) -> tuple[float, float, float]:
    deltas = [100.0 * (after - before) / before for before, after in zip(baseline, candidate)]
    estimate = statistics.median(deltas)
    rng = random.Random(0xB12D6E)
    samples = []
    for _ in range(10_000):
        samples.append(statistics.median(rng.choices(deltas, k=len(deltas))))
    samples.sort()
    return estimate, samples[249], samples[9749]


def report(pairs: list[tuple[dict, dict]]) -> dict:
    if len(pairs) < 3:
        raise ValueError("at least three interleaved pairs are required")
    identity_fields = ("image_sha256", "vars_sha256", "config_sha256", "host_model", "macos_version", "power_source")
    reference = pairs[0][0]
    for before, after in pairs:
        for sample in (before, after):
            for field in identity_fields:
                if sample.get(field) != reference.get(field):
                    raise ValueError(f"mismatched {field}")
    baseline_hashes = {before["binary_hash"] for before, _ in pairs}
    candidate_hashes = {after["binary_hash"] for _, after in pairs}
    if len(baseline_hashes) != 1 or len(candidate_hashes) != 1:
        raise ValueError("each side must use one sealed binary")
    before_values = [float(before["desktop_elapsed_ms"]) for before, _ in pairs]
    after_values = [float(after["desktop_elapsed_ms"]) for _, after in pairs]
    estimate, lower, upper = paired_interval(before_values, after_values)
    return {
        "mode": "AA" if baseline_hashes == candidate_hashes else "AB",
        "pairs": len(pairs),
        "baseline_binary_sha256": next(iter(baseline_hashes)),
        "candidate_binary_sha256": next(iter(candidate_hashes)),
        "baseline_samples_ms": before_values,
        "candidate_samples_ms": after_values,
        "baseline_p50_ms": statistics.median(before_values),
        "candidate_p50_ms": statistics.median(after_values),
        "baseline_p95_ms": percentile(before_values, 0.95),
        "candidate_p95_ms": percentile(after_values, 0.95),
        "paired_median_delta_ms": statistics.median([a - b for b, a in zip(before_values, after_values)]),
        "paired_median_delta_percent": estimate,
        "paired_median_delta_percent_ci95": [lower, upper],
        "improvement_interval_entirely_beneficial": upper < 0.0,
    }


def self_test() -> None:
    common = {
        "pass": True, "valid": True, "image_sha256": "a" * 64,
        "vars_sha256": "b" * 64, "config_sha256": "c" * 64,
        "host_model": "MacTest", "macos_version": "26.0", "power_source": "AC Power",
    }
    pairs = []
    for before, after in ((100, 90), (110, 99), (120, 108)):
        left = dict(common, binary_hash="d" * 64, desktop_elapsed_ms=before)
        right = dict(common, binary_hash="e" * 64, desktop_elapsed_ms=after)
        pairs.append((left, right))
    result = report(pairs)
    assert result["mode"] == "AB" and result["paired_median_delta_percent"] == -10.0
    assert result["improvement_interval_entirely_beneficial"]
    print("HVF boot performance A/B reporter self-test: PASS")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pair", nargs=2, action="append", metavar=("BASELINE", "CANDIDATE"))
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return 0
    if not args.pair:
        parser.error("at least three --pair BASELINE CANDIDATE arguments are required")
    pairs = [(receipt(left), receipt(right)) for left, right in args.pair]
    print(json.dumps(report(pairs), indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
