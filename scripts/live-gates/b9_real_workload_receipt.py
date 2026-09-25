#!/usr/bin/env python3
"""Strict private/public B9 pilot receipts and owned cleanup verification."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import re
import sys

from b9_real_workload_inputs import KEYS, stable_file
from b9_real_workload_observation import (CLASSES, collector_identity,
                                          finished_identity, read_guest_json,
                                          ready_identity, scanout_hashes)
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from b9_workload_diagnostic import summarize_csv

TIER = "d9-b9-real-workload"
SHA = re.compile(r"[0-9a-f]{64}\Z")
SOURCE = re.compile(r"[0-9a-f]{40}\Z")
JOB = re.compile(r"[A-Za-z0-9][A-Za-z0-9._-]{0,127}\Z")
FLAGS = ("pass", "claim_eligible", "criterion_pass", "capability_promotion")
PUBLIC_KEYS = frozenset(("schema", "tier", "criterion", "candidate_id", "job_id", "commit",
                         "input_manifest_sha256", "sealed_binary_sha256", "asset_hashes",
                         "outcome", "result_class", "pilot_count", "required_workload_count",
                         "cleanup_complete", "source_integrity", "frame_count",
                         "scanout_sample_count", "distinct_scanout_count", "csv_sha256",
                         "target_pid", "cpu_start_span_ms", "p50_nearest_rank_ms",
                         "p95_nearest_rank_ms", "p99_nearest_rank_ms",
                         "guest_shutdown_observed", "driver_umd_sha256",
                         "nonce_sha256", "ready_sha256",
                         "collector_sha256", "finished_sha256", *FLAGS))


def _unique(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("duplicate B9 receipt key")
        result[key] = value
    return result


def read_json(path: Path, limit: int = 65536) -> dict:
    size, _ = stable_file(path, maximum=limit)
    with path.open("rb") as stream:
        raw = stream.read(limit + 1)
    if len(raw) != size:
        raise ValueError("B9 receipt changed during read")
    try:
        value = json.loads(raw.decode("utf-8"), object_pairs_hook=_unique,
                           parse_constant=lambda _: (_ for _ in ()).throw(ValueError("nonfinite JSON")))
    except (UnicodeError, json.JSONDecodeError) as error:
        raise ValueError("malformed B9 receipt") from error
    if not isinstance(value, dict):
        raise ValueError("B9 receipt must be an object")
    return value


def job_fields(directory: Path) -> dict:
    path = directory / "job.env"
    size, _ = stable_file(path, maximum=8192)
    fields = {}
    for line in path.read_text(encoding="ascii").splitlines():
        key, separator, value = line.partition("=")
        if not separator or key in fields or not re.fullmatch(r"[a-z0-9_]+", key):
            raise ValueError("malformed or duplicate B9 job field")
        fields[key] = value
    if size == 0 or fields.get("tier") != TIER or not JOB.fullmatch(fields.get("job_id", "")):
        raise ValueError("B9 job identity differs")
    if not SOURCE.fullmatch(fields.get("commit", "")):
        raise ValueError("B9 job source must be exact SHA")
    for key in ("input_manifest_sha256", "sealed_binary_sha256",
                *("asset_" + name + "_sha256" for name in KEYS)):
        if not SHA.fullmatch(fields.get(key, "")):
            raise ValueError("B9 job asset seal absent: " + key)
    return fields


def _plain(value: dict, job: dict, public: bool) -> None:
    if public and set(value) != PUBLIC_KEYS:
        raise ValueError("public B9 receipt has missing or additional fields")
    if (value.get("schema") != "bridgevm.b9-real-workload-pilot.v1"
            or value.get("tier") != TIER or value.get("criterion") != "B9"
            or value.get("candidate_id") != "vlc-3.0.23-arm64-kodi-bbb-1080p-av1-10s-v1"
            or any(value.get(key) != job[key] for key in
                   ("job_id", "commit", "input_manifest_sha256", "sealed_binary_sha256"))
            or any(value.get(flag) is not False for flag in FLAGS)):
        raise ValueError("B9 receipt claims or sealed identity differ")
    assets = value.get("asset_hashes")
    if not isinstance(assets, dict) or set(assets) != KEYS:
        raise ValueError("B9 receipt asset set differs")
    for name, digest in assets.items():
        if digest != job["asset_" + name + "_sha256"]:
            raise ValueError("B9 asset differs from queue seal: " + name)
    if (value.get("outcome") not in {"diagnostic-complete", "diagnostic-incomplete", "canceled"}
            or value.get("result_class") not in CLASSES
            or type(value.get("pilot_count")) is not int
            or value["pilot_count"] not in (0, 1)
            or type(value.get("required_workload_count")) is not int
            or value["required_workload_count"] != 20
            or any(type(value.get(key)) is not bool for key in
                   ("cleanup_complete", "source_integrity"))
            or any(type(value.get(key)) is not int or value[key] < 0 for key in
                   ("frame_count", "scanout_sample_count"))
            or not SHA.fullmatch(value.get("nonce_sha256", ""))):
        raise ValueError("B9 diagnostic count or outcome differs")
    for key in ("ready_sha256", "collector_sha256", "finished_sha256", "csv_sha256",
                "driver_umd_sha256"):
        digest = value.get(key, "")
        if not isinstance(digest, str) or (digest and not SHA.fullmatch(digest)):
            raise ValueError("B9 observation hash malformed")
    if value["result_class"] == "VISIBLE_PLAYBACK_COMPLETE":
        if (value["outcome"] != "diagnostic-complete" or value["pilot_count"] != 1
                or value["cleanup_complete"] is not True
                or value["source_integrity"] is not True
                or value.get("guest_shutdown_observed") is not True
                or not SHA.fullmatch(value.get("driver_umd_sha256", ""))
                or value["frame_count"] < 2 or value["scanout_sample_count"] < 3
                or value.get("distinct_scanout_count", 0) < 3
                or not SHA.fullmatch(value.get("csv_sha256", ""))):
            raise ValueError("B9 complete label lacks live diagnostic evidence")
        metrics = [value.get(key) for key in
                   ("p50_nearest_rank_ms", "p95_nearest_rank_ms", "p99_nearest_rank_ms")]
        if (any(type(number) not in (int, float) or not math.isfinite(number) or number <= 0
                for number in metrics) or metrics != sorted(metrics)
                or type(value.get("cpu_start_span_ms")) not in (int, float)
                or value["cpu_start_span_ms"] < 7000):
            raise ValueError("B9 complete label lacks bounded frame metrics")
    if public:
        def check_pathless(item):
            if isinstance(item, str) and ("/" in item or "\\" in item or ":" in item):
                raise ValueError("path or guest content leaked into public B9 receipt")
            if isinstance(item, dict):
                for nested in item.values(): check_pathless(nested)
            if isinstance(item, list):
                for nested in item: check_pathless(nested)
        check_pathless(value)


def validate_private(value: dict, job: dict, diagnostic: Path | None = None) -> None:
    _plain(value, job, public=False)
    if (type(value.get("owned_vm_pgid")) is not int or value["owned_vm_pgid"] < 0
            or value.get("owned_process_group_stopped") is not True
            or value.get("cleanup_complete") is not True):
        raise ValueError("B9 owned process or media cleanup is unproved")
    work = value.get("owned_work_path")
    if work is not None and (not isinstance(work, str) or not work.endswith(
            "/b9-pilot-" + job["job_id"])):
        raise ValueError("B9 owned work path differs")
    if value["result_class"] == "VISIBLE_PLAYBACK_COMPLETE" and (
            work is None or value["owned_vm_pgid"] <= 1):
        raise ValueError("B9 visible label lacks owned work and VM process identity")
    if diagnostic is not None:
        root = value.get("private_artifact_dir", "raw")
        if root not in ("raw", "raw/guest"):
            raise ValueError("B9 private artifact directory differs")
        artifacts = value.get("private_artifacts", {})
        if not isinstance(artifacts, dict) or len(artifacts) > 8:
            raise ValueError("B9 private artifact list differs")
        for name, seal in artifacts.items():
            if name not in {"run.log", "launcher.log", "virtio-gpu.jsonl", "guest-ready.json",
                            "guest-collector.json", "guest-finished.json", "presentmon.csv"}:
                raise ValueError("unknown B9 private artifact")
            path = diagnostic / root / name
            if not isinstance(seal, dict) or set(seal) != {"bytes", "sha256"}:
                raise ValueError("B9 private artifact seal differs")
            if stable_file(path) != (seal["bytes"], seal["sha256"]):
                raise ValueError("B9 private artifact changed: " + name)
        paths = value.get("scanout_files", [])
        hashes = value.get("scanout_hashes", [])
        if not isinstance(paths, list) or not isinstance(hashes, list) or len(paths) != len(hashes):
            raise ValueError("B9 scanout seal count differs")
        for name, digest in zip(paths, hashes):
            if not isinstance(name, str) or not re.fullmatch(r"scanout-[0-4]/presented[.]ppm", name):
                raise ValueError("unsafe B9 scanout artifact name")
            if sha256(diagnostic / "raw" / name) != digest:
                raise ValueError("B9 scanout artifact changed")
        if value["result_class"] == "VISIBLE_PLAYBACK_COMPLETE":
            verify_visible_raw(value, diagnostic)


def verify_visible_raw(value: dict, diagnostic: Path) -> None:
    """Recompute the successful label from retained bytes, not receipt numbers."""
    root = diagnostic / value["private_artifact_dir"]
    required = ("guest-ready.json", "guest-collector.json", "guest-finished.json",
                "presentmon.csv", "run.log")
    if any(name not in value["private_artifacts"] for name in required):
        raise ValueError("B9 visible label lacks retained guest/CSV/run evidence")
    ready, ready_hash = read_guest_json(root / "guest-ready.json")
    nonce = ready.get("nonce")
    if (not isinstance(nonce, str) or not re.fullmatch(r"[0-9a-f]{32}", nonce)
            or hashlib.sha256(nonce.encode()).hexdigest() != value["nonce_sha256"]
            or ready_hash != value["ready_sha256"]):
        raise ValueError("B9 ready nonce or bytes differ")
    pins = {"media_sha256": value["asset_hashes"]["media"],
            "vlc_zip_sha256": value["asset_hashes"]["vlc_zip"],
            "presentmon_sha256": value["asset_hashes"]["presentmon"],
            "expected_driver_umd_sha256": value["driver_umd_sha256"]}
    pid, hwnd = ready_identity(ready, nonce, pins)
    collector, collector_hash = read_guest_json(root / "guest-collector.json")
    session = collector_identity(collector, nonce, pid)
    finished, finished_hash = read_guest_json(root / "guest-finished.json")
    finished_identity(finished, nonce, pid, hwnd)
    if (collector_hash != value["collector_sha256"]
            or finished_hash != value["finished_sha256"]
            or finished["failure_code"] != "none"
            or finished["window_visible"] is not True
            or finished["direct3d11_module_loaded"] is not True
            or finished["av1_decoder_module_loaded"] is not True
            or finished["driver_umd_sha256"] != value["driver_umd_sha256"]
            or finished["vlc_exit_observed"] is not True
            or finished["vlc_exit_code"] != 0
            or finished["collector_started"] is not True
            or finished["collector_exit_code"] != 0
            or finished["playback_elapsed_ms"] < 8000):
        raise ValueError("B9 VLC/collector raw completion differs")
    csv = root / "presentmon.csv"
    size, digest = stable_file(csv, maximum=7_500_000)
    if size != finished["csv_bytes"] or digest != finished["csv_sha256"]:
        raise ValueError("B9 retained CSV differs from guest finish")
    metrics = summarize_csv(csv, digest, pid)
    if (metrics["cpu_start_span_ms"] < 7000
            or any(value.get(key) != metrics[key] for key in
                   ("csv_sha256", "frame_count", "target_pid", "swapchain_address",
                    "cpu_start_span_ms", "p50_nearest_rank_ms", "p95_nearest_rank_ms",
                    "p99_nearest_rank_ms"))
            or value.get("window_handle") != hwnd
            or value.get("collector_session") != session
            or value.get("playback_elapsed_ms") != finished["playback_elapsed_ms"]
            or value.get("vlc_exit_code") != 0 or value.get("collector_exit_code") != 0):
        raise ValueError("B9 receipt frame metrics or identity differ from raw data")
    frame_files = [diagnostic / "raw" / name for name in value["scanout_files"]]
    hashes, captures = scanout_hashes(frame_files)
    if (len(hashes) < 3 or len(set(hashes)) < 3
            or hashes != value["scanout_hashes"]
            or captures != value.get("scanout_capture_hashes")
            or value.get("scanout_sample_count") != len(hashes)
            or value.get("distinct_scanout_count") != len(set(hashes))):
        raise ValueError("B9 live scanout distinction differs from retained captures")
    run_log = (root / "run.log").read_bytes().decode("utf-8", errors="replace").replace("\r", "\n")
    if (run_log.count("live input accepted: command=Key(") != 1
            or f"BVAGENT WINFOCUS {hwnd} -> OK WINFOCUS" not in run_log
            or f"B9-FOREGROUND-{hwnd}" not in run_log
            or "stop: PSCI SYSTEM_OFF" not in run_log):
        raise ValueError("B9 foreground input or clean shutdown absent from raw log")


def sha256(path: Path) -> str:
    return stable_file(path)[1]


def public_view(value: dict, job: dict) -> dict:
    public = {key: value.get(key, "" if key.endswith("sha256") else 0)
              for key in PUBLIC_KEYS}
    for key in ("guest_shutdown_observed",):
        public[key] = value.get(key) is True
    for key in ("asset_hashes", "schema", "tier", "criterion", "candidate_id", "job_id",
                "commit", "outcome", "result_class", "input_manifest_sha256",
                "sealed_binary_sha256"):
        public[key] = value[key]
    for flag in FLAGS:
        public[flag] = False
    _plain(public, job, public=True)
    return public


def cleanup_guard(directory: Path, commit: str, job_id: str) -> None:
    directory = directory.resolve()
    job = job_fields(directory)
    if job["commit"] != commit or job["job_id"] != job_id:
        raise ValueError("B9 cleanup job identity differs")
    receipt = read_json(directory / "receipt.json")
    validate_private(receipt, job, directory / "diagnostic")
    work = receipt.get("owned_work_path")
    expected_work = (Path.home() / "BridgeVM/work").resolve() / ("b9-pilot-" + job_id)
    if work is not None and Path(work) != expected_work:
        raise ValueError("B9 owned work root differs from the private lane")
    if expected_work.exists() or expected_work.is_symlink():
        raise ValueError("B9 writable clones or share remain after runner")
    pgid = receipt["owned_vm_pgid"]
    if pgid > 1:
        try:
            os.killpg(pgid, 0)
        except ProcessLookupError:
            pass
        else:
            raise ValueError("B9 owned VM process group is still alive")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("verify-private", "verify-public", "guard"))
    parser.add_argument("job_dir", type=Path)
    parser.add_argument("commit")
    parser.add_argument("job_id", nargs="?")
    args = parser.parse_args()
    try:
        args.job_dir = args.job_dir.resolve()
        job = job_fields(args.job_dir)
        if job["commit"] != args.commit:
            raise ValueError("B9 source seal differs")
        if args.mode == "guard":
            if args.job_id is None:
                raise ValueError("B9 guard requires exact job id")
            cleanup_guard(args.job_dir, args.commit, args.job_id)
        else:
            path = args.job_dir / ("receipt.json" if args.mode == "verify-private"
                                       else "receipt.public.json")
            value = read_json(path)
            if args.mode == "verify-private":
                validate_private(value, job, args.job_dir / "diagnostic")
            else:
                _plain(value, job, public=True)
    except (OSError, ValueError) as error:
        print("B9 receipt refused: " + str(error), file=sys.stderr)
        return 2
    print("B9 real-workload diagnostic receipt: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
