#!/usr/bin/env python3
"""Write one path-free, provenance-complete T15 receipt from sealed values."""

from __future__ import annotations

import datetime
import json
import os
import sys
from pathlib import Path


def required(name: str) -> str:
    value = os.environ.get(name, "")
    if not value:
        raise ValueError(f"missing receipt field {name}")
    return value


def truth(name: str) -> bool:
    value = required(name)
    if value not in {"true", "false"}:
        raise ValueError(f"invalid Boolean receipt field {name}")
    return value == "true"


def main() -> int:
    if len(sys.argv) != 2:
        raise SystemExit("usage: write-hvf-boot-performance-receipt.py PATH")
    passed = truth("PERF_RECEIPT_PASS")
    desktop = os.environ.get("PERF_RECEIPT_DESKTOP", "")
    receipt = {
        "schema_version": 2,
        "tier": "t15-hvf-boot-performance",
        "gate_id": "hvf-boot-performance-diagnostic",
        "job_id": required("PERF_JOB_ID"),
        "commit": required("PERF_HARNESS_COMMIT"),
        "harness_commit": required("PERF_HARNESS_COMMIT"),
        "tested_commit": required("PERF_BINARY_SOURCE_COMMIT"),
        "binary_source_commit": required("PERF_BINARY_SOURCE_COMMIT"),
        "binary_profile": required("PERF_BINARY_PROFILE"),
        "binary_features": required("PERF_BINARY_FEATURES"),
        "rust_toolchain": required("PERF_RUST_TOOLCHAIN"),
        "binary_hash": required("PERF_BINARY_HASH"),
        "input_manifest_sha256": required("PERF_MANIFEST_HASH"),
        "image_sha256": required("PERF_IMAGE_HASH"),
        "vars_sha256": required("PERF_VARS_HASH"),
        "firmware_sha256": required("PERF_FIRMWARE_HASH"),
        "config_sha256": required("PERF_CONFIG_HASH"),
        "campaign_id": required("PERF_CAMPAIGN_ID"),
        "campaign_mode": required("PERF_CAMPAIGN_MODE"),
        "campaign_role": required("PERF_CAMPAIGN_ROLE"),
        "campaign_ordinal": int(required("PERF_CAMPAIGN_ORDINAL")),
        "campaign_expected_runs": int(required("PERF_CAMPAIGN_EXPECTED_RUNS")),
        "host_model": required("PERF_HOST_MODEL"),
        "macos_version": required("PERF_MACOS_VERSION"),
        "power_source": required("PERF_POWER_SOURCE_START"),
        "power_source_start": required("PERF_POWER_SOURCE_START"),
        "power_source_end": required("PERF_POWER_SOURCE_END"),
        "smp_cpus": 4,
        "ram_mib": 6144,
        "desktop_elapsed_ms": float(desktop) if desktop else None,
        "valid": truth("PERF_RECEIPT_VALID"),
        "invalid_reason": os.environ.get("PERF_RECEIPT_REASON", ""),
        "sample_count": 1,
        "run_count": 1,
        "required_run_count": 1,
        "passes": 1 if passed else 0,
        "failures": 0 if passed else 1,
        "started_at": required("PERF_STARTED_AT"),
        "finished_at": datetime.datetime.now(datetime.timezone.utc).isoformat().replace("+00:00", "Z"),
        "outcome": required("PERF_RECEIPT_OUTCOME"),
        "pass": passed,
        "evidence_paths": ["boot/boot-timer-report.tsv"],
        "known_confounders": ["full clone integrity hash immediately precedes boot (warm cache)"],
    }
    path = Path(sys.argv[1])
    path.write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
