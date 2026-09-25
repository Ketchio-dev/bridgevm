"""Bind one guest VLC process, collector CSV, and real host scanout samples."""

from __future__ import annotations

import hashlib
import json
from pathlib import Path
import re

from b9_real_workload_inputs import stable_file
import sys
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from b9_workload_diagnostic import DiagnosticError, summarize_csv, unique_pairs

NONCE = re.compile(r"[0-9a-f]{32}\Z")
SHA = re.compile(r"[0-9a-f]{64}\Z")
CLASSES = frozenset(("VLC_PID_PRESENTS_CAPTURED", "PLAYBACK_INCOMPLETE",
                     "PRESENTATION_UNPROVEN", "COLLECTOR_FAILED",
                     "GUEST_NOT_READY", "INVALID_EVIDENCE"))


def read_guest_json(path: Path, limit: int = 8192) -> tuple[dict, str]:
    size, digest = stable_file(path, maximum=limit)
    with path.open("rb") as stream:
        raw = stream.read(limit + 1)
    if len(raw) != size or hashlib.sha256(raw).hexdigest() != digest:
        raise ValueError("guest observation changed during read")
    try:
        value = json.loads(raw.decode("utf-8"), object_pairs_hook=unique_pairs)
    except (UnicodeError, json.JSONDecodeError, DiagnosticError) as error:
        raise ValueError("malformed guest observation") from error
    if not isinstance(value, dict):
        raise ValueError("guest observation must be an object")
    return value, digest


def _int(value: object, *, minimum: int = 0) -> bool:
    return type(value) is int and minimum <= value < 2**63


def ready_identity(value: dict, nonce: str, pins: dict[str, str]) -> tuple[int, int]:
    if (value.get("schema") != "bridgevm.b9-vlc-ready.v1" or value.get("nonce") != nonce
            or not _int(value.get("pid"), minimum=1)
            or not _int(value.get("hwnd"), minimum=1)
            or not isinstance(value.get("title"), str)
            or not 0 < len(value["title"]) <= 256
            or any(value.get(field) != digest for field, digest in pins.items())
            or set(value) != {"schema", "nonce", "pid", "hwnd", "title", *pins}):
        raise ValueError("guest VLC ready identity differs")
    return value["pid"], value["hwnd"]


def collector_identity(value: dict, nonce: str, pid: int) -> str:
    session = "BridgeVM-B9-" + nonce
    if (set(value) != {"schema", "nonce", "pid", "session", "collector_pid"}
            or value.get("schema") != "bridgevm.b9-collector-ready.v1"
            or value.get("nonce") != nonce or value.get("pid") != pid
            or value.get("session") != session
            or not _int(value.get("collector_pid"), minimum=1)
            or value["collector_pid"] == pid):
        raise ValueError("PresentMon session is not bound to VLC PID")
    return session


def finished_identity(value: dict, nonce: str, pid: int, hwnd: int) -> None:
    keys = {"schema", "nonce", "pid", "hwnd", "window_visible",
            "vlc_exit_observed", "vlc_exit_code", "playback_elapsed_ms",
            "collector_started", "collector_exit_code", "csv_sha256", "csv_bytes",
            "direct3d11_module_loaded", "av1_decoder_module_loaded", "driver_umd_sha256",
            "failure_code", "failure_detail"}
    if (set(value) != keys or value.get("schema") != "bridgevm.b9-vlc-finished.v1"
            or value.get("nonce") != nonce or value.get("pid") != pid
            or value.get("hwnd") != hwnd
            or any(type(value.get(key)) is not bool for key in
                   ("window_visible", "vlc_exit_observed", "collector_started",
                    "direct3d11_module_loaded", "av1_decoder_module_loaded"))
            or any(not _int(value.get(key), minimum=(-1 if key.endswith("code") else 0))
                   for key in ("vlc_exit_code", "playback_elapsed_ms", "collector_exit_code",
                               "csv_bytes"))
            or not isinstance(value.get("failure_code"), str)
            or value["failure_code"] not in {"none", "PREPARATION_FAILED", "COLLECTOR_FAILED",
                                               "PLAYBACK_INCOMPLETE"}
            or not isinstance(value.get("failure_detail"), str)
            or len(value["failure_detail"]) > 160
            or not isinstance(value.get("csv_sha256"), str)
            or (value["csv_sha256"] != "" and not SHA.fullmatch(value["csv_sha256"]))
            or not isinstance(value.get("driver_umd_sha256"), str)
            or (value["driver_umd_sha256"] != "" and not SHA.fullmatch(value["driver_umd_sha256"]))):
        raise ValueError("guest VLC finish identity differs")


def scanout_hashes(frames: list[Path]) -> tuple[list[str], list[str]]:
    if len(frames) > 8:
        raise ValueError("too many scanout samples")
    digests, capture_digests = [], []
    for path in frames:
        if path.suffix != ".ppm":
            raise ValueError("scanout sample must be PPM")
        size, digest = stable_file(path, maximum=32_000_000)
        if size < 1024:
            raise ValueError("empty scanout sample")
        capture = path.with_name("capture.env")
        capture_size, capture_digest = stable_file(capture, maximum=2048)
        with capture.open("rb") as stream:
            raw = stream.read(2049)
        if len(raw) != capture_size:
            raise ValueError("scanout capture metadata changed")
        fields = {}
        for line in raw.decode("ascii").splitlines():
            key, sep, value = line.partition("=")
            if not sep or key in fields:
                raise ValueError("malformed scanout capture metadata")
            fields[key] = value
        if (set(fields) != {"source", "iosurface_id", "width", "height",
                            "initial_seed", "captured_seed", "nonblack_pixels",
                            "bgra_sha256", "ppm_sha256"}
                or fields["source"] != "active-cgl-iosurface"
                or fields["initial_seed"] == fields["captured_seed"]
                or not SHA.fullmatch(fields["bgra_sha256"])
                or fields["ppm_sha256"] != digest):
            raise ValueError("scanout capture did not authenticate a new frame")
        try:
            width, height = int(fields["width"]), int(fields["height"])
            if not (640 <= width <= 3840 and 480 <= height <= 2160
                    and 0 < int(fields["nonblack_pixels"]) <= width * height):
                raise ValueError("scanout geometry or nonblack pixels differ")
        except ValueError as error:
            raise ValueError("invalid scanout capture dimensions") from error
        header = f"P6\n{width} {height}\n255\n".encode()
        if size != len(header) + width * height * 3:
            raise ValueError("scanout PPM payload length differs")
        with path.open("rb") as stream:
            if stream.read(len(header)) != header:
                raise ValueError("scanout PPM header differs")
            rgb = stream.read()
        bgra_path = path.with_name("presented.bgra")
        bgra_size, bgra_hash = stable_file(bgra_path, maximum=36_000_000)
        if bgra_size != width * height * 4 or bgra_hash != fields["bgra_sha256"]:
            raise ValueError("scanout BGRA frame differs from capture metadata")
        bgra = bgra_path.read_bytes()
        converted = bytearray(width * height * 3)
        converted[0::3], converted[1::3], converted[2::3] = (
            bgra[2::4], bgra[1::4], bgra[0::4])
        if rgb != converted:
            raise ValueError("scanout PPM does not match captured BGRA")
        digests.append(digest)
        capture_digests.append(capture_digest)
    return digests, capture_digests


def observe(share: Path, nonce: str, pins: dict[str, str], frames: list[Path]) -> dict:
    if not NONCE.fullmatch(nonce) or set(pins) != {
            "media_sha256", "vlc_zip_sha256", "presentmon_sha256",
            "expected_driver_umd_sha256"}:
        raise ValueError("invalid B9 pilot binding")
    result = {"result_class": "GUEST_NOT_READY", "ready_sha256": "",
              "collector_sha256": "", "finished_sha256": "", "csv_sha256": "",
              "target_pid": 0, "frame_count": 0, "scanout_sample_count": 0,
              "distinct_scanout_count": 0}
    ready_path = share / ("ready-" + nonce + ".json")
    if not ready_path.exists():
        return result
    ready, result["ready_sha256"] = read_guest_json(ready_path)
    pid, hwnd = ready_identity(ready, nonce, pins)
    result["target_pid"] = pid
    result["window_handle"] = hwnd
    result["result_class"] = "COLLECTOR_FAILED"
    collector_path = share / ("collector-" + nonce + ".json")
    if not collector_path.exists():
        return result
    collector, result["collector_sha256"] = read_guest_json(collector_path)
    result["collector_session"] = collector_identity(collector, nonce, pid)
    result["result_class"] = "PLAYBACK_INCOMPLETE"
    finished_path = share / ("finished-" + nonce + ".json")
    if not finished_path.exists():
        return result
    finished, result["finished_sha256"] = read_guest_json(finished_path)
    finished_identity(finished, nonce, pid, hwnd)
    result["guest_failure_code"] = finished["failure_code"]
    result["vlc_exit_code"] = finished["vlc_exit_code"]
    result["collector_exit_code"] = finished["collector_exit_code"]
    result["playback_elapsed_ms"] = finished["playback_elapsed_ms"]
    result["scanout_hashes"], result["scanout_capture_hashes"] = scanout_hashes(frames)
    result["scanout_sample_count"] = len(frames)
    result["distinct_scanout_count"] = len(set(result["scanout_hashes"]))
    if (finished["collector_started"] is not True or finished["collector_exit_code"] != 0
            or not SHA.fullmatch(finished["csv_sha256"]) or finished["csv_bytes"] <= 0):
        result["result_class"] = "COLLECTOR_FAILED"
        return result
    csv = share / ("b9-" + nonce + ".csv")
    size, digest = stable_file(csv, maximum=7_500_000)
    if size != finished["csv_bytes"] or digest != finished["csv_sha256"]:
        raise ValueError("PresentMon CSV changed after guest finish")
    try:
        metrics = summarize_csv(csv, digest, pid)
    except DiagnosticError:
        result["result_class"] = "COLLECTOR_FAILED"
        return result
    result.update({key: metrics[key] for key in (
        "csv_sha256", "frame_count", "swapchain_address", "cpu_start_span_ms",
        "p50_nearest_rank_ms", "p95_nearest_rank_ms", "p99_nearest_rank_ms")})
    if (finished["failure_code"] != "none" or finished["vlc_exit_observed"] is not True
            or finished["vlc_exit_code"] != 0 or finished["playback_elapsed_ms"] < 8000
            or metrics["cpu_start_span_ms"] < 7000):
        return result
    if (finished["window_visible"] is not True
            or finished["direct3d11_module_loaded"] is not True
            or finished["av1_decoder_module_loaded"] is not True
            or finished["driver_umd_sha256"] != pins["expected_driver_umd_sha256"]
            or result["distinct_scanout_count"] < 3):
        result["result_class"] = "PRESENTATION_UNPROVEN"
        return result
    result["result_class"] = "VLC_PID_PRESENTS_CAPTURED"
    return result
