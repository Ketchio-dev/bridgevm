#!/usr/bin/env python3
"""Finalize and validate the private T16 v2 host-environment sample logs."""

from __future__ import annotations

import argparse
import json
import tempfile
from pathlib import Path


THERMAL_SCHEMA = "bridgevm.hvf-nvme-host-thermal.v2"
HID_SCHEMA = "bridgevm.hvf-nvme-host-hid.v2"
INTERVAL_SECONDS = 5
MIN_HID_IDLE_NS = 300_000_000_000


class EvidenceError(ValueError):
    pass


def _raw_rows(path: Path, label: str) -> list[tuple[int, int]]:
    if not path.is_file() or path.is_symlink():
        raise EvidenceError(f"{label} raw log is not a regular file")
    rows: list[tuple[int, int]] = []
    for line_number, line in enumerate(path.read_text(encoding="ascii").splitlines(), 1):
        fields = line.split("\t")
        if len(fields) != 2 or not all(field.isdecimal() for field in fields):
            raise EvidenceError(f"{label} raw row {line_number} is malformed")
        ordinal, value = map(int, fields)
        if ordinal != len(rows) + 1:
            raise EvidenceError(f"{label} raw ordinals are not contiguous")
        rows.append((ordinal, value))
    if not rows:
        raise EvidenceError(f"{label} raw log is empty")
    return rows


def _write_new(path: Path, document: dict[str, object]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("x", encoding="utf-8") as output:
        json.dump(document, output, sort_keys=True, separators=(",", ":"))
        output.write("\n")


def finalize(thermal_raw: Path, hid_raw: Path, thermal_json: Path, hid_json: Path) -> None:
    thermal = _raw_rows(thermal_raw, "thermal")
    hid = _raw_rows(hid_raw, "HID")
    if len(thermal) != len(hid):
        raise EvidenceError("thermal and HID sample counts differ")
    _write_new(
        thermal_json,
        {
            "sample_interval_seconds": INTERVAL_SECONDS,
            "samples": [
                {"ordinal": ordinal, "thermal_state": state}
                for ordinal, state in thermal
            ],
            "schema": THERMAL_SCHEMA,
        },
    )
    _write_new(
        hid_json,
        {
            "sample_interval_seconds": INTERVAL_SECONDS,
            "samples": [
                {"idle_nanoseconds": idle, "ordinal": ordinal}
                for ordinal, idle in hid
            ],
            "schema": HID_SCHEMA,
        },
    )


def _load(path: Path, schema: str, value_key: str) -> list[int]:
    if not path.is_file() or path.is_symlink():
        raise EvidenceError(f"{schema} log is not a regular file")
    document = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(document, dict) or set(document) != {
        "schema",
        "sample_interval_seconds",
        "samples",
    }:
        raise EvidenceError(f"{schema} log has unexpected fields")
    if document["schema"] != schema or document["sample_interval_seconds"] != INTERVAL_SECONDS:
        raise EvidenceError(f"{schema} identity is invalid")
    samples = document["samples"]
    if not isinstance(samples, list) or not samples:
        raise EvidenceError(f"{schema} samples are missing")
    values: list[int] = []
    for index, sample in enumerate(samples, 1):
        if (
            not isinstance(sample, dict)
            or set(sample) != {"ordinal", value_key}
            or sample["ordinal"] != index
            or type(sample[value_key]) is not int
            or sample[value_key] < 0
        ):
            raise EvidenceError(f"{schema} sample {index} is invalid")
        values.append(sample[value_key])
    return values


def summarize(thermal_path: Path, hid_path: Path) -> dict[str, object]:
    thermal = _load(thermal_path, THERMAL_SCHEMA, "thermal_state")
    hid = _load(hid_path, HID_SCHEMA, "idle_nanoseconds")
    if len(thermal) != len(hid):
        raise EvidenceError("thermal and HID finalized sample counts differ")
    resets = sum(current < previous for previous, current in zip(hid, hid[1:]))
    nominal = sum(state == 0 for state in thermal)
    reasons: list[str] = []
    if nominal != len(thermal):
        reasons.append("thermal-not-nominal")
    if hid[0] < MIN_HID_IDLE_NS:
        reasons.append("host-hid-not-idle-at-start")
    if resets:
        reasons.append("host-hid-reset")
    return {
        "failure_reason": ",".join(reasons) if reasons else "none",
        "host_hid_idle_end_seconds": round(hid[-1] / 1_000_000_000, 6),
        "host_hid_idle_start_seconds": round(hid[0] / 1_000_000_000, 6),
        "host_hid_reset_count": resets,
        "thermal_nominal_samples": nominal,
        "thermal_sample_count": len(thermal),
        "valid": not reasons,
    }


def _self_test() -> None:
    with tempfile.TemporaryDirectory(prefix="bridgevm-t16-v2-environment-") as temporary:
        root = Path(temporary)
        thermal_raw = root / "thermal.raw"
        hid_raw = root / "hid.raw"
        thermal_raw.write_text("1\t0\n2\t0\n", encoding="ascii")
        hid_raw.write_text("1\t300000000000\n2\t305000000000\n", encoding="ascii")
        thermal_json = root / "thermal.json"
        hid_json = root / "hid.json"
        finalize(thermal_raw, hid_raw, thermal_json, hid_json)
        good = summarize(thermal_json, hid_json)
        assert good["valid"] is True and good["host_hid_reset_count"] == 0
        bad_hid = json.loads(hid_json.read_text(encoding="utf-8"))
        bad_hid["samples"][1]["idle_nanoseconds"] = 1
        (root / "bad-hid.json").write_text(json.dumps(bad_hid), encoding="utf-8")
        bad = summarize(thermal_json, root / "bad-hid.json")
        assert bad["valid"] is False and bad["host_hid_reset_count"] == 1
        bad_thermal = json.loads(thermal_json.read_text(encoding="utf-8"))
        bad_thermal["samples"][0]["thermal_state"] = 1
        (root / "bad-thermal.json").write_text(json.dumps(bad_thermal), encoding="utf-8")
        assert summarize(root / "bad-thermal.json", hid_json)["valid"] is False
        try:
            finalize(thermal_raw, hid_raw, thermal_json, root / "other.json")
        except FileExistsError:
            pass
        else:
            raise AssertionError("existing finalized output was overwritten")
    print("HVF NVMe v2 host environment self-test: PASS")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    finalize_parser = subparsers.add_parser("finalize")
    for option in ("thermal-raw", "hid-raw", "thermal-json", "hid-json"):
        finalize_parser.add_argument(f"--{option}", type=Path, required=True)
    summarize_parser = subparsers.add_parser("summarize")
    summarize_parser.add_argument("--thermal-json", type=Path, required=True)
    summarize_parser.add_argument("--hid-json", type=Path, required=True)
    subparsers.add_parser("self-test")
    args = parser.parse_args()
    try:
        if args.command == "finalize":
            finalize(args.thermal_raw, args.hid_raw, args.thermal_json, args.hid_json)
        elif args.command == "summarize":
            print(json.dumps(summarize(args.thermal_json, args.hid_json), sort_keys=True))
        else:
            _self_test()
    except (EvidenceError, json.JSONDecodeError, OSError) as error:
        parser.exit(1, f"HVF NVMe v2 host environment: {error}\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
