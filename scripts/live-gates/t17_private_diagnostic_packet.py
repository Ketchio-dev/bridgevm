#!/usr/bin/env python3
"""Retain and independently verify a bounded, private T17 first-READY packet."""
from __future__ import annotations

import argparse
from contextlib import ExitStack
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import struct
import sys

SCHEMA = "bridgevm.t17-private-diagnostic-packet.v1"
STAMP_SCHEMA = "bridgevm.windows-hvf-3d-off-product-e2e-host-stamp.v1"
LANE_SCHEMA = "bridgevm.windows-hvf-3d-off-product-e2e-lane.v2"
REQUEST_SCHEMA = "bridgevm.windows-hvf-3d-off-product-e2e-request.v2"
LOG_CAP = 16 * 1024 * 1024
FRAME_CAP = 64 * 1024 * 1024
DISPLAY_CAP = 64 * 1024 * 1024
TOTAL_CAP = 256 * 1024 * 1024
JSON_CAP = 1024 * 1024
INDEX_CAP = 64 * 1024
CHUNK = 1024 * 1024
DIR_FLAGS = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC
READ_FLAGS = os.O_RDONLY | os.O_NOFOLLOW | os.O_CLOEXEC | os.O_NONBLOCK
WRITE_FLAGS = os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW | os.O_CLOEXEC
ROLES = ("run_log", "final_raw", "final_ppm", "display_fb")
CAPS = dict(zip(ROLES, (LOG_CAP, FRAME_CAP, FRAME_CAP, DISPLAY_CAP)))
HEX = re.compile(r"[0-9a-f]{64}\Z")
COMMIT = re.compile(r"[0-9a-f]{40}\Z")
JOB = re.compile(r"[A-Za-z0-9][A-Za-z0-9._-]{0,127}\Z")
FRAME = re.compile(r"(ramfb|virtio-gpu)-[0-9]+x[0-9]+-[0-9a-f]+-[0-9a-f]{16}\Z")
ACK = re.compile(r"^HOST-DIAGNOSTIC-STOP: generation=([0-9]+) nonce=([0-9a-f]{32}) request consumed; ending run through final report$", re.M)
COMPLETE_DETAIL = re.compile(r"host_stop=status=complete,generation=([0-9]+),nonce=([0-9a-f]{32}),report=complete,helper=terminal,log_offset=([0-9]+)(?:;|\Z)")
HOST_STATUS = re.compile(r"host_stop=status=(complete|missing|incomplete)(?:,|;|\Z)")
FIRST_READY_DETAIL = "first boot has no BVAGENT READY/PONG evidence;"


class CaptureError(ValueError):
    pass


def unique(pairs: list[tuple[str, object]]) -> dict:
    result = {}
    for key, value in pairs:
        if key in result:
            raise CaptureError("duplicate JSON field")
        result[key] = value
    return result


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def fingerprint(info: os.stat_result) -> tuple[int, ...]:
    return (info.st_dev, info.st_ino, info.st_uid, info.st_mode, info.st_nlink,
            info.st_size, info.st_mtime_ns, info.st_ctime_ns)


def node_identity(info: os.stat_result) -> tuple[int, ...]:
    return (info.st_dev, info.st_ino, info.st_uid, info.st_mode, info.st_nlink, info.st_size)


def owned_directory(fd: int, *, device: int | None = None, private: bool = False) -> os.stat_result:
    info = os.fstat(fd)
    if not stat.S_ISDIR(info.st_mode) or info.st_uid != os.geteuid():
        raise CaptureError("unsafe owned directory")
    if device is not None and info.st_dev != device:
        raise CaptureError("source directory crossed a device")
    if private and stat.S_IMODE(info.st_mode) != 0o700:
        raise CaptureError("private directory is not mode 0700")
    return info


def open_absolute_directory(path: Path, stack: ExitStack) -> int:
    if not path.is_absolute() or str(path) != os.path.normpath(str(path)):
        raise CaptureError("directory path is not canonical")
    fd = os.open("/", DIR_FLAGS)
    stack.callback(os.close, fd)
    for component in path.parts[1:]:
        parent = fd
        before = os.stat(component, dir_fd=parent, follow_symlinks=False)
        fd = os.open(component, DIR_FLAGS, dir_fd=parent)
        stack.callback(os.close, fd)
        if fingerprint(before) != fingerprint(os.fstat(fd)):
            raise CaptureError("directory changed while opening")
    return fd


def child_directory(parent: int, name: str, device: int, stack: ExitStack) -> int | None:
    try:
        before = os.stat(name, dir_fd=parent, follow_symlinks=False)
    except FileNotFoundError:
        return None
    fd = os.open(name, DIR_FLAGS, dir_fd=parent)
    stack.callback(os.close, fd)
    if fingerprint(before) != fingerprint(os.fstat(fd)):
        raise CaptureError("source directory changed while opening")
    owned_directory(fd, device=device)
    return fd


def request_consumed(evidence: int) -> bool:
    try:
        os.stat("diagnostic-stop.request", dir_fd=evidence, follow_symlinks=False)
    except FileNotFoundError:
        return True
    return False


def source_file(parent: int, name: str, device: int, stack: ExitStack) -> tuple[int, os.stat_result] | None:
    try:
        before = os.stat(name, dir_fd=parent, follow_symlinks=False)
    except FileNotFoundError:
        return None
    if (not stat.S_ISREG(before.st_mode) or before.st_uid != os.geteuid()
            or before.st_dev != device or before.st_nlink != 1 or before.st_size < 0):
        raise CaptureError("unsafe source file")
    fd = os.open(name, READ_FLAGS, dir_fd=parent)
    stack.callback(os.close, fd)
    if fingerprint(before) != fingerprint(os.fstat(fd)):
        raise CaptureError("source file changed while opening")
    return fd, before


def unchanged(parent: int, name: str, fd: int, before: os.stat_result, *, mutable: bool = False) -> None:
    after = os.fstat(fd)
    by_name = os.stat(name, dir_fd=parent, follow_symlinks=False)
    compare = node_identity if mutable else fingerprint
    if compare(before) != compare(after) or compare(before) != compare(by_name):
        raise CaptureError("source file changed while capturing")


def read_exact(fd: int, offset: int, size: int) -> bytes:
    chunks = []
    while size:
        chunk = os.pread(fd, min(size, CHUNK), offset)
        if not chunk:
            raise CaptureError("source file had a short read")
        chunks.append(chunk)
        offset += len(chunk)
        size -= len(chunk)
    return b"".join(chunks)


def read_json(parent: int, name: str, device: int, stack: ExitStack, *, cap: int = JSON_CAP) -> tuple[dict, str]:
    opened = source_file(parent, name, device, stack)
    if opened is None:
        raise CaptureError("authenticated JSON is missing")
    fd, before = opened
    if before.st_size == 0 or before.st_size > cap:
        raise CaptureError("authenticated JSON is empty or oversized")
    raw = read_exact(fd, 0, before.st_size)
    unchanged(parent, name, fd, before)
    try:
        value = json.loads(raw.decode("utf-8"), object_pairs_hook=unique)
    except (UnicodeError, json.JSONDecodeError) as error:
        raise CaptureError("authenticated JSON is malformed") from error
    if not isinstance(value, dict):
        raise CaptureError("authenticated JSON is not an object")
    return value, digest(raw)


def destination(private: int, name: str, stack: ExitStack, created: list[str]) -> int:
    fd = os.open(name, WRITE_FLAGS, 0o600, dir_fd=private)
    stack.callback(os.close, fd)
    created.append(name)
    info = os.fstat(fd)
    if (not stat.S_ISREG(info.st_mode) or info.st_uid != os.geteuid()
            or info.st_nlink != 1 or stat.S_IMODE(info.st_mode) != 0o600
            or info.st_dev != os.fstat(private).st_dev):
        raise CaptureError("unsafe private destination")
    return fd


def write_all(fd: int, data: bytes) -> None:
    while data:
        written = os.write(fd, data)
        if written <= 0:
            raise CaptureError("private destination had a short write")
        data = data[written:]


def copy_file(source: tuple[int, os.stat_result], source_dir: int, source_name: str,
              private: int, dest_name: str, offset: int, count: int,
              stack: ExitStack, created: list[str], *, keep_bytes: bool = False,
              mutable: bool = False) -> tuple[str, bytes]:
    fd, before = source
    if count < 0 or offset < 0 or offset + count > before.st_size:
        raise CaptureError("copy range is outside source")
    output = destination(private, dest_name, stack, created)
    checksum = hashlib.sha256()
    captured = []
    position = offset
    remaining = count
    while remaining:
        block = os.pread(fd, min(remaining, CHUNK), position)
        if not block:
            raise CaptureError("source file had a short read")
        write_all(output, block)
        checksum.update(block)
        if keep_bytes:
            captured.append(block)
        remaining -= len(block)
        position += len(block)
    os.fsync(output)
    unchanged(source_dir, source_name, fd, before, mutable=mutable)
    if os.fstat(output).st_size != count:
        raise CaptureError("private destination size differs")
    return checksum.hexdigest(), b"".join(captured)


def entry(role: str, source: str, status: str = "missing", *, reason: str = "absent",
          original_size: int | None = None) -> dict:
    return {"role": role, "source": source, "status": status, "reason": reason,
            "file": None, "bytes": 0, "original_size": original_size,
            "offset": None, "truncated": False, "sha256": None}


def retained(item: dict, name: str, count: int, offset: int, checksum: str) -> None:
    item.update(status="retained", reason="none", file=name, bytes=count, offset=offset,
                truncated=(offset != 0 or count != item["original_size"]), sha256=checksum)


def terminal_report(tail: bytes, detail: str, tail_offset: int) -> tuple[int, str] | None:
    claim = COMPLETE_DETAIL.search(detail)
    if claim is None or int(claim[3]) < tail_offset or int(claim[3]) > tail_offset + len(tail):
        return None
    text = tail[int(claim[3]) - tail_offset:].decode("utf-8", errors="replace")
    ack = next((match for match in ACK.finditer(text) if match[2] == claim[2]), None)
    if ack is None or ack[1] != claim[1]:
        return None
    serial = text.find("\n--- serial (tail) ---\n", ack.end())
    banner = text.find("\n=== EDK2 boot probe (with Apple hv_gic) ===\n", ack.end(), serial)
    stop = text.find("\nstop: host diagnostic stop requested\n", banner, serial)
    if serial < 0 or banner < 0 or stop < 0 or not text.rstrip("\n").endswith("\n--- end ---"):
        return None
    return int(claim[1]), text[stop:serial]


def final_frame_names(tail: bytes, frame_dir: str, detail: str, tail_offset: int) -> tuple[str, str] | None:
    parsed = terminal_report(tail, detail, tail_offset)
    if parsed is None:
        return None
    report = parsed[1]
    raw = re.findall(r"^ramfb framebuffer artifact: raw=(.+)$", report, re.M)
    ppm = re.findall(r"^ramfb framebuffer artifact: ppm=(.+)$", report, re.M)
    if len(raw) != 1 or len(ppm) != 1:
        return None
    result = []
    for path, suffix in ((raw[0], ".xrgb8888"), (ppm[0], ".ppm")):
        name = os.path.basename(path)
        if path != f"{frame_dir}/{name}" or not name.endswith(suffix) or not FRAME.fullmatch(name.removesuffix(suffix)):
            raise CaptureError("final report named an unsafe framebuffer artifact")
        result.append(name)
    if result[0].removesuffix(".xrgb8888") != result[1].removesuffix(".ppm"):
        raise CaptureError("final report framebuffer pair differs")
    return result[0], result[1]


def observed_generation(detail: str, tail: bytes, tail_offset: int) -> int | None:
    parsed = terminal_report(tail, detail, tail_offset)
    return parsed[0] if parsed is not None else None


def display_header(header: bytes, size: int) -> tuple[int, int] | None:
    if len(header) != 64:
        return None
    magic, version, width, height, stride, fourcc = struct.unpack_from("<6I", header)
    sequence = struct.unpack_from("<Q", header, 24)[0]
    if (magic != 0x42564642 or version != 1 or fourcc != 0x34325258
            or not (0 < width <= 16384 and 0 < height <= 16384)
            or stride < width * 4 or stride % 4 or sequence == 0 or sequence & 1):
        return None
    count = 64 + height * stride
    if count > DISPLAY_CAP or count > size:
        return None
    return sequence, count


def capture(args: argparse.Namespace) -> None:
    if not JOB.fullmatch(args.job_id) or not COMMIT.fullmatch(args.commit) or args.lane not in (1, 2, 3):
        raise CaptureError("invalid packet identity")
    lane_root = str(args.lane_root)
    if not re.fullmatch(r"/(?:private/)?tmp/bridgevm-e2e-" + re.escape(args.job_id) + r"\.[A-Za-z0-9]{6}/lane-" + str(args.lane), lane_root):
        raise CaptureError("lane root is outside its job boundary")
    created: list[str] = []
    with ExitStack() as stack:
        private = open_absolute_directory(args.private, stack)
        private_info = owned_directory(private, private=True)
        lane = open_absolute_directory(args.lane_root, stack)
        lane_info = owned_directory(lane)
        request, request_sha = read_json(lane, "request.json", lane_info.st_dev, stack)
        result_name = f"lane-{args.lane}-result.json"
        stamp_name = f"lane-{args.lane}-authenticated.json"
        result, result_sha = read_json(private, result_name, private_info.st_dev, stack)
        stamp, stamp_sha = read_json(private, stamp_name, private_info.st_dev, stack)
        nonce = result.get("nonce")
        slug = f"bridgevm-t17-lane-{args.lane}-{str(nonce)[:12]}"
        fixed = {"job_id": args.job_id, "commit": args.commit, "lane": args.lane, "nonce": nonce}
        detail = result.get("failure_detail")
        statuses = HOST_STATUS.findall(detail) if isinstance(detail, str) else []
        if (not isinstance(nonce, str) or not HEX.fullmatch(nonce)
                or any(request.get(key) != value or result.get(key) != value or stamp.get(key) != value for key, value in fixed.items())
                or request.get("schema_version") != REQUEST_SCHEMA or result.get("schema_version") != LANE_SCHEMA
                or request.get("lane_root") != lane_root or request.get("vm_slug") != slug
                or request.get("campaign_mode") != args.mode or result.get("campaign_mode") != args.mode
                or result.get("failure_code") != "guest-evidence-missing" or result.get("first_ready") is not False
                or not isinstance(detail, str) or not detail.startswith(FIRST_READY_DETAIL)
                or len(statuses) != 1
                or stamp.get("schema_version") != STAMP_SCHEMA
                or stamp.get("request_sha256") != request_sha or stamp.get("result_sha256") != result_sha
                or set(stamp) != {"schema_version", "job_id", "commit", "lane", "nonce", "request_sha256", "result_sha256"}):
            raise CaptureError("packet source does not match authenticated first-READY failure")
        evidence_rel = f"library/{slug}/bundle.vmbridge/logs/hvf"
        evidence_path = f"{lane_root}/{evidence_rel}"
        frame_path = f"{evidence_path}/ramfb"
        items = {role: entry(role, source) for role, source in (
            ("run_log", f"{evidence_rel}/run.log"),
            ("final_raw", f"{evidence_rel}/ramfb/<final>.xrgb8888"),
            ("final_ppm", f"{evidence_rel}/ramfb/<final>.ppm"),
            ("display_fb", f"{evidence_rel}/display.fb"))}
        total = 0
        try:
            evidence = lane
            for component in ("library", slug, "bundle.vmbridge", "logs", "hvf"):
                evidence = child_directory(evidence, component, lane_info.st_dev, stack) if evidence is not None else None
            tail = b""
            tail_offset = 0
            if evidence is not None:
                if statuses[0] == "complete" and not request_consumed(evidence):
                    raise CaptureError("claimed host stop request was not consumed")
                log = source_file(evidence, "run.log", lane_info.st_dev, stack)
                if log is not None:
                    size = log[1].st_size
                    offset = max(0, size - LOG_CAP)
                    tail_offset = offset
                    count = size - offset
                    checksum, tail = copy_file(log, evidence, "run.log", private,
                        f"t17-diagnostic-lane-{args.lane}-run-log.bin", offset, count, stack, created,
                        keep_bytes=True)
                    items["run_log"]["original_size"] = size
                    retained(items["run_log"], created[-1], count, offset, checksum)
                    total += count
                frames = final_frame_names(tail, frame_path, detail, tail_offset)
                if frames:
                    for role, name in zip(("final_raw", "final_ppm"), frames):
                        items[role]["source"] = f"{evidence_rel}/ramfb/{name}"
                frame_dir = child_directory(evidence, "ramfb", lane_info.st_dev, stack) if frames else None
                if frames and frame_dir is not None:
                    sources = [source_file(frame_dir, name, lane_info.st_dev, stack) for name in frames]
                    for role, name, source in zip(("final_raw", "final_ppm"), frames, sources):
                        if source is not None:
                            items[role]["original_size"] = source[1].st_size
                            if source[1].st_size > FRAME_CAP:
                                items[role].update(status="oversize", reason="64-mib-cap")
                    if all(source is not None and source[1].st_size <= FRAME_CAP for source in sources):
                        for role, name, source in zip(("final_raw", "final_ppm"), frames, sources):
                            size = source[1].st_size
                            dest_name = f"t17-diagnostic-lane-{args.lane}-{role}{'.ppm' if role == 'final_ppm' else '.xrgb8888'}"
                            checksum, _ = copy_file(source, frame_dir, name, private, dest_name,
                                                    0, size, stack, created)
                            retained(items[role], dest_name, size, 0, checksum)
                            total += size
                    else:
                        for role in ("final_raw", "final_ppm"):
                            if items[role]["status"] == "missing":
                                items[role]["reason"] = "pair-incomplete"
                display = source_file(evidence, "display.fb", lane_info.st_dev, stack)
                if display is not None:
                    fd, info = display
                    item = items["display_fb"]
                    item["original_size"] = info.st_size
                    if info.st_size > DISPLAY_CAP:
                        item.update(status="oversize", reason="64-mib-cap")
                    elif info.st_size >= 64:
                        before = read_exact(fd, 0, 64)
                        validated = display_header(before, info.st_size)
                        if validated is None:
                            item.update(status="unavailable", reason="invalid-or-odd-frame")
                        else:
                            sequence, count = validated
                            dest_name = f"t17-diagnostic-lane-{args.lane}-display.fb"
                            checksum, _ = copy_file(display, evidence, "display.fb", private,
                                                    dest_name, 0, count, stack, created, mutable=True)
                            after = read_exact(fd, 0, 64)
                            if after != before or struct.unpack_from("<Q", after, 24)[0] != sequence:
                                os.unlink(dest_name, dir_fd=private)
                                created.remove(dest_name)
                                item.update(status="unavailable", reason="sequence-changed")
                            else:
                                retained(item, dest_name, count, 0, checksum)
                                total += count
                    else:
                        item.update(status="unavailable", reason="short-header")
            if total > TOTAL_CAP:
                raise CaptureError("packet exceeds 256-MiB total cap")
            index = {"schema_version": SCHEMA, "job_id": args.job_id, "commit": args.commit,
                     "campaign_mode": args.mode, "lane": args.lane, "lane_root": lane_root,
                     "lane_identity": f"{lane_info.st_dev}:{lane_info.st_ino}", "nonce": nonce,
                     "request_sha256": request_sha, "result_sha256": result_sha,
                     "stamp_sha256": stamp_sha, "host_stop_status": statuses[0],
                     "observed_generation": observed_generation(detail, tail, tail_offset),
                     "display_generation": "unattributed",
                     "windows_function_symbols": "unavailable: exact-build mapping unverified",
                     "total_bytes": total, "artifacts": [items[role] for role in ROLES]}
            body = (json.dumps(index, sort_keys=True, indent=2) + "\n").encode("utf-8")
            if len(body) > INDEX_CAP:
                raise CaptureError("private index exceeds its cap")
            output = destination(private, f"t17-diagnostic-lane-{args.lane}-index.json", stack, created)
            write_all(output, body)
            os.fsync(output)
            verify(args)
        except Exception:
            for name in reversed(created):
                os.unlink(name, dir_fd=private)
            raise


def verify(args: argparse.Namespace) -> None:
    with ExitStack() as stack:
        private = open_absolute_directory(args.private, stack)
        info = owned_directory(private, private=True)
        index, _ = read_json(private, f"t17-diagnostic-lane-{args.lane}-index.json", info.st_dev, stack, cap=INDEX_CAP)
        result, result_sha = read_json(private, f"lane-{args.lane}-result.json", info.st_dev, stack)
        stamp, stamp_sha = read_json(private, f"lane-{args.lane}-authenticated.json", info.st_dev, stack)
        expected_keys = {"schema_version", "job_id", "commit", "campaign_mode", "lane", "lane_root",
                     "lane_identity", "nonce", "request_sha256", "result_sha256", "stamp_sha256",
                     "host_stop_status", "observed_generation", "display_generation",
                     "windows_function_symbols", "total_bytes", "artifacts"}
        detail = result.get("failure_detail")
        statuses = HOST_STATUS.findall(detail) if isinstance(detail, str) else []
        expected_stamp_keys = {"schema_version", "job_id", "commit", "lane", "nonce", "request_sha256", "result_sha256"}
        if (set(index) != expected_keys or index["schema_version"] != SCHEMA
                or index["job_id"] != args.job_id or index["commit"] != args.commit
                or index["campaign_mode"] != args.mode or index["lane"] != args.lane
                or result.get("schema_version") != LANE_SCHEMA
                or result.get("job_id") != args.job_id or result.get("commit") != args.commit
                or result.get("campaign_mode") != args.mode or result.get("lane") != args.lane
                or result.get("failure_code") != "guest-evidence-missing" or result.get("first_ready") is not False
                or not isinstance(detail, str) or not detail.startswith(FIRST_READY_DETAIL)
                or len(statuses) != 1 or index["host_stop_status"] != statuses[0]
                or set(stamp) != expected_stamp_keys or stamp.get("schema_version") != STAMP_SCHEMA
                or stamp.get("job_id") != args.job_id or stamp.get("commit") != args.commit
                or stamp.get("lane") != args.lane or stamp.get("nonce") != result.get("nonce")
                or not isinstance(stamp.get("request_sha256"), str) or not HEX.fullmatch(stamp["request_sha256"])
                or not isinstance(index["lane_root"], str)
                or not re.fullmatch(r"/(?:private/)?tmp/bridgevm-e2e-" + re.escape(args.job_id) + r"\.[A-Za-z0-9]{6}/lane-" + str(args.lane), index["lane_root"])
                or not re.fullmatch(r"[0-9]+:[0-9]+", str(index["lane_identity"]))
                or not isinstance(index["nonce"], str) or not HEX.fullmatch(index["nonce"])
                or index["nonce"] != result.get("nonce") or index["nonce"] != stamp.get("nonce")
                or index["result_sha256"] != result_sha or index["stamp_sha256"] != stamp_sha
                or index["request_sha256"] != stamp.get("request_sha256")
                or stamp.get("result_sha256") != result_sha
                or index["display_generation"] != "unattributed"
                or index["windows_function_symbols"] != "unavailable: exact-build mapping unverified"
                or not (index["observed_generation"] is None or type(index["observed_generation"]) is int and index["observed_generation"] >= 0)
                or type(index["total_bytes"]) is not int):
            raise CaptureError("private index identity or schema differs")
        expected_role_names = {
            "run_log": f"t17-diagnostic-lane-{args.lane}-run-log.bin",
            "final_raw": f"t17-diagnostic-lane-{args.lane}-final_raw.xrgb8888",
            "final_ppm": f"t17-diagnostic-lane-{args.lane}-final_ppm.ppm",
            "display_fb": f"t17-diagnostic-lane-{args.lane}-display.fb",
        }
        artifacts = index["artifacts"]
        if not isinstance(artifacts, list) or len(artifacts) != len(ROLES):
            raise CaptureError("private index has incomplete roles")
        source_base = f"library/bridgevm-t17-lane-{args.lane}-{index['nonce'][:12]}/bundle.vmbridge/logs/hvf"
        total = 0
        tail = b""
        for role, item in zip(ROLES, artifacts):
            if (not isinstance(item, dict)
                    or set(item) != {"role", "source", "status", "reason", "file", "bytes", "original_size", "offset", "truncated", "sha256"}
                    or item["role"] != role or not isinstance(item["source"], str)
                    or item["status"] not in {"retained", "missing", "oversize", "unavailable"}
                    or not isinstance(item["reason"], str)
                    or type(item["bytes"]) is not int or item["bytes"] < 0
                    or item["original_size"] is not None and (type(item["original_size"]) is not int or item["original_size"] < 0)
                    or type(item["truncated"]) is not bool):
                raise CaptureError("private index artifact schema differs")
            if role in ("run_log", "display_fb"):
                if item["source"] != f"{source_base}/{'run.log' if role == 'run_log' else 'display.fb'}":
                    raise CaptureError("private index source role differs")
            else:
                suffix = ".xrgb8888" if role == "final_raw" else ".ppm"
                placeholder = f"{source_base}/ramfb/<final>{suffix}"
                if item["source"] != placeholder:
                    prefix = f"{source_base}/ramfb/"
                    if (not item["source"].startswith(prefix)
                            or not FRAME.fullmatch(item["source"][len(prefix):].removesuffix(suffix))
                            or not item["source"].endswith(suffix)):
                        raise CaptureError("private index framebuffer source differs")
            if item["status"] == "retained":
                if (item["file"] != expected_role_names[role] or item["reason"] != "none"
                        or type(item["offset"]) is not int or item["offset"] < 0
                        or type(item["original_size"]) is not int
                        or item["bytes"] > CAPS[role] or item["offset"] + item["bytes"] > item["original_size"]
                        or item["truncated"] != (item["offset"] != 0 or item["bytes"] != item["original_size"])
                        or not isinstance(item["sha256"], str) or not HEX.fullmatch(item["sha256"])):
                    raise CaptureError("private index retained entry differs")
                source = source_file(private, item["file"], info.st_dev, stack)
                if source is None or source[1].st_size != item["bytes"] or stat.S_IMODE(source[1].st_mode) != 0o600:
                    raise CaptureError("private artifact is missing or unsafe")
                actual = hashlib.sha256()
                retained_bytes = [] if role == "run_log" else None
                position = 0
                while position < item["bytes"]:
                    block = os.pread(source[0], min(CHUNK, item["bytes"] - position), position)
                    if not block:
                        raise CaptureError("private artifact had a short read")
                    actual.update(block)
                    if retained_bytes is not None:
                        retained_bytes.append(block)
                    position += len(block)
                unchanged(private, item["file"], source[0], source[1])
                if actual.hexdigest() != item["sha256"]:
                    raise CaptureError("private artifact SHA-256 differs")
                if retained_bytes is not None:
                    tail = b"".join(retained_bytes)
                if role == "display_fb":
                    header = display_header(read_exact(source[0], 0, 64), item["bytes"])
                    if header is None or header[1] != item["bytes"]:
                        raise CaptureError("private display framebuffer header differs")
                total += item["bytes"]
            elif (item["file"] is not None or item["bytes"] != 0 or item["offset"] is not None
                  or item["truncated"] or item["sha256"] is not None):
                raise CaptureError("unavailable artifact claims retained bytes")
            elif item["status"] == "oversize" and (item["original_size"] is None or item["original_size"] <= CAPS[role]):
                raise CaptureError("oversize artifact is within cap")
            elif item["status"] == "missing" and item["reason"] not in {"absent", "pair-incomplete"}:
                raise CaptureError("missing artifact reason differs")
            elif item["status"] == "unavailable" and item["reason"] not in {"invalid-or-odd-frame", "sequence-changed", "short-header"}:
                raise CaptureError("unavailable artifact reason differs")
        if total != index["total_bytes"] or total > TOTAL_CAP:
            raise CaptureError("private packet total differs")
        if (artifacts[1]["status"] == "retained") != (artifacts[2]["status"] == "retained"):
            raise CaptureError("final framebuffer pair is incomplete")
        tail_offset = artifacts[0]["offset"] or 0
        if observed_generation(detail, tail, tail_offset) != index["observed_generation"]:
            raise CaptureError("observed generation differs from retained log")
        if index["host_stop_status"] == "complete":
            if index["observed_generation"] is None or terminal_report(tail, detail, tail_offset) is None:
                raise CaptureError("complete host stop lacks a terminal owner report")
        elif index["observed_generation"] is not None:
            raise CaptureError("incomplete host stop claims a generation")
        frame_sources = (artifacts[1]["source"], artifacts[2]["source"])
        if all("<final>" not in source for source in frame_sources):
            if frame_sources[0].removesuffix(".xrgb8888") != frame_sources[1].removesuffix(".ppm"):
                raise CaptureError("private index framebuffer pair source differs")
        elif any("<final>" not in source for source in frame_sources):
            raise CaptureError("private index framebuffer pair source is incomplete")
        frame_dir = f"{index['lane_root']}/{source_base}/ramfb"
        names = final_frame_names(tail, frame_dir, detail, tail_offset)
        if names is None and any(item["status"] == "retained" for item in artifacts[1:3]):
            raise CaptureError("private frame lacks a terminal report")
        if names is None and any("<final>" not in source for source in frame_sources):
            raise CaptureError("private frame source lacks a terminal report")
        if names is not None and frame_sources != tuple(f"{source_base}/ramfb/{name}" for name in names):
            raise CaptureError("private frame differs from terminal report")
        if args.lane_root is not None:
            if str(args.lane_root) != index["lane_root"]:
                raise CaptureError("private index lane root differs")
            lane = open_absolute_directory(args.lane_root, stack)
            lane_info = owned_directory(lane)
            if f"{lane_info.st_dev}:{lane_info.st_ino}" != index["lane_identity"]:
                raise CaptureError("private index lane identity differs")
            request, request_sha = read_json(lane, "request.json", lane_info.st_dev, stack)
            if (request_sha != index["request_sha256"]
                    or request.get("schema_version") != REQUEST_SCHEMA
                    or request.get("job_id") != args.job_id or request.get("commit") != args.commit
                    or request.get("campaign_mode") != args.mode or request.get("lane") != args.lane
                    or request.get("nonce") != index["nonce"]
                    or request.get("lane_root") != index["lane_root"]):
                raise CaptureError("private index request binding differs")
            if index["host_stop_status"] == "complete":
                evidence = lane
                slug = f"bridgevm-t17-lane-{args.lane}-{index['nonce'][:12]}"
                for component in ("library", slug, "bundle.vmbridge", "logs", "hvf"):
                    evidence = child_directory(evidence, component, lane_info.st_dev, stack) if evidence is not None else None
                if evidence is None or not request_consumed(evidence):
                    raise CaptureError("claimed host stop request was not consumed")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("operation", choices=("capture", "verify"))
    parser.add_argument("--private", type=Path, required=True)
    parser.add_argument("--lane-root", type=Path)
    parser.add_argument("--job-id", required=True)
    parser.add_argument("--commit", required=True)
    parser.add_argument("--campaign-mode", dest="mode_name", choices=("pilot", "release"), required=True)
    parser.add_argument("--lane", type=int, required=True)
    args = parser.parse_args()
    args.mode = args.mode_name
    try:
        if args.mode_name not in ("pilot", "release") or not JOB.fullmatch(args.job_id) or not COMMIT.fullmatch(args.commit) or args.lane not in (1, 2, 3):
            raise CaptureError("invalid packet identity")
        if args.operation == "capture":
            if args.lane_root is None:
                raise CaptureError("capture requires lane root")
            capture(args)
        else:
            verify(args)
    except (CaptureError, OSError) as error:
        print(f"T17 private diagnostic packet refused: {type(error).__name__}: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
