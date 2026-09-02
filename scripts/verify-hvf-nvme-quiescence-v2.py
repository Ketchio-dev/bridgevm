#!/usr/bin/env python3
"""Validate the nonce-bound guest quiescence artifact for T16 v2."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import statistics
import tempfile
from pathlib import Path
from typing import Any


CONFIG_SCHEMA = "bridgevm.hvf-nvme-quiescence.v1"
RESULT_SCHEMA = "bridgevm.hvf-nvme-quiescence-result.v1"
EXPECTED_CONFIG = {
    "schema": CONFIG_SCHEMA,
    "samples": 30,
    "interval_seconds": 1,
    "cpu_percent": {"median_max": 10, "p95_max": 20},
    "disk_bytes_per_second": {"median_max": 1_048_576, "p95_max": 4_194_304},
    "disk_queue_length": {"p95_max": 0.25},
    "post_ready_settle_seconds": 120,
    "post_sample_quiet_seconds": 15,
}


class EvidenceError(ValueError):
    pass


def _regular_bytes(path: Path, label: str) -> bytes:
    if not path.is_file() or path.is_symlink():
        raise EvidenceError(f"{label} is not a regular file")
    payload = path.read_bytes()
    if not payload or len(payload) >= 8 * 1024 * 1024 or payload.startswith(b"\xef\xbb\xbf"):
        raise EvidenceError(f"{label} violates the private artifact boundary")
    return payload


def _document(path: Path, label: str) -> tuple[dict[str, Any], str]:
    payload = _regular_bytes(path, label)
    try:
        document = json.loads(payload.decode("utf-8"))
    except (UnicodeError, json.JSONDecodeError) as error:
        raise EvidenceError(f"{label} is not UTF-8 JSON") from error
    if not isinstance(document, dict):
        raise EvidenceError(f"{label} is not one JSON object")
    return document, hashlib.sha256(payload).hexdigest()


def _number(value: Any, label: str) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise EvidenceError(f"{label} is not numeric")
    result = float(value)
    if not math.isfinite(result) or result < 0:
        raise EvidenceError(f"{label} is not finite and nonnegative")
    return result


def _p95(values: list[float]) -> float:
    ordered = sorted(values)
    return ordered[math.ceil(0.95 * len(ordered)) - 1]


def validate(result_path: Path, config_path: Path, nonce: str) -> dict[str, object]:
    if len(nonce) != 32 or any(character not in "0123456789abcdef" for character in nonce):
        raise EvidenceError("nonce is not 32 lowercase hexadecimal characters")
    config, config_hash = _document(config_path, "quiescence config")
    if config != EXPECTED_CONFIG:
        raise EvidenceError("quiescence config is not the exact fixed policy")
    result, result_hash = _document(result_path, "quiescence result")
    if set(result) != {
        "schema",
        "nonce",
        "config_sha256",
        "security_services_enabled",
        "samples",
    }:
        raise EvidenceError("quiescence result has unexpected fields")
    if result["schema"] != RESULT_SCHEMA or result["nonce"] != nonce:
        raise EvidenceError("quiescence result identity is invalid")
    if result["config_sha256"] != config_hash:
        raise EvidenceError("quiescence result does not seal the exact config")
    if result["security_services_enabled"] is not True:
        raise EvidenceError("Windows security services are not enabled")
    samples = result["samples"]
    if not isinstance(samples, list) or len(samples) != 30:
        raise EvidenceError("quiescence result does not contain exactly 30 samples")
    cpu: list[float] = []
    disk: list[float] = []
    queue: list[float] = []
    keys = {"ordinal", "cpu_percent", "disk_bytes_per_second", "disk_queue_length"}
    for ordinal, sample in enumerate(samples, 1):
        if not isinstance(sample, dict) or set(sample) != keys or sample["ordinal"] != ordinal:
            raise EvidenceError(f"quiescence sample {ordinal} is malformed")
        cpu_value = _number(sample["cpu_percent"], f"sample {ordinal} CPU")
        if cpu_value > 100:
            raise EvidenceError(f"sample {ordinal} CPU exceeds 100 percent")
        cpu.append(cpu_value)
        disk.append(_number(sample["disk_bytes_per_second"], f"sample {ordinal} disk rate"))
        queue.append(_number(sample["disk_queue_length"], f"sample {ordinal} queue"))
    summary: dict[str, object] = {
        "guest_cpu_median_percent": statistics.median(cpu),
        "guest_cpu_p95_percent": _p95(cpu),
        "guest_disk_bps_median": statistics.median(disk),
        "guest_disk_bps_p95": _p95(disk),
        "guest_disk_queue_p95": _p95(queue),
        "guest_quiescence_config_sha256": config_hash,
        "guest_quiescence_log_sha256": result_hash,
        "guest_quiescence_sample_count": len(samples),
    }
    summary["valid"] = bool(
        summary["guest_cpu_median_percent"] <= 10
        and summary["guest_cpu_p95_percent"] <= 20
        and summary["guest_disk_bps_median"] <= 1_048_576
        and summary["guest_disk_bps_p95"] <= 4_194_304
        and summary["guest_disk_queue_p95"] <= 0.25
    )
    return summary


def _self_test() -> None:
    with tempfile.TemporaryDirectory(prefix="bridgevm-t16-v2-quiescence-") as temporary:
        root = Path(temporary)
        config = root / "config.json"
        config.write_text(json.dumps(EXPECTED_CONFIG, sort_keys=True, separators=(",", ":")) + "\n")
        nonce = "1" * 32
        samples = [
            {
                "ordinal": ordinal,
                "cpu_percent": 2.0,
                "disk_bytes_per_second": 1024.0,
                "disk_queue_length": 0.01,
            }
            for ordinal in range(1, 31)
        ]
        result = root / "result.json"
        document = {
            "schema": RESULT_SCHEMA,
            "nonce": nonce,
            "config_sha256": hashlib.sha256(config.read_bytes()).hexdigest(),
            "security_services_enabled": True,
            "samples": samples,
        }
        result.write_text(json.dumps(document, separators=(",", ":")) + "\n")
        assert validate(result, config, nonce)["valid"] is True
        document["samples"][28]["cpu_percent"] = 21
        document["samples"][29]["cpu_percent"] = 21
        result.write_text(json.dumps(document, separators=(",", ":")) + "\n")
        assert validate(result, config, nonce)["valid"] is False
        document["samples"][0]["ordinal"] = 2
        result.write_text(json.dumps(document, separators=(",", ":")) + "\n")
        try:
            validate(result, config, nonce)
        except EvidenceError:
            pass
        else:
            raise AssertionError("malformed ordinal was accepted")
    print("HVF NVMe v2 guest quiescence self-test: PASS")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--result", type=Path)
    parser.add_argument("--config", type=Path)
    parser.add_argument("--nonce")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        _self_test()
        return 0
    if not args.result or not args.config or not args.nonce:
        parser.error("--result, --config and --nonce are required")
    try:
        summary = validate(args.result, args.config, args.nonce)
    except (EvidenceError, OSError) as error:
        parser.exit(2, f"HVF NVMe v2 guest quiescence: {error}\n")
    print(json.dumps(summary, sort_keys=True, separators=(",", ":")))
    return 0 if summary["valid"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
