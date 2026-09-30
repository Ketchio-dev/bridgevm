#!/usr/bin/env python3
"""Admit a T17 private packet by kind and list a post-READY lane's host share.

A first-READY packet needs the fixed no-READY detail with exactly one host-stop
claim. A post-READY packet needs a lane that proved first READY and then failed
guest-evidence-missing, with at most one claim. Its share listing records names,
types, sizes and SHA-256 only; links are never followed and no share bytes are
retained. A share problem is recorded in the listing rather than raised, so the
share cannot discard the packet, and an error on one entry is that entry's reason.
"""
from __future__ import annotations

import errno
import hashlib
import json
import os
import re
import stat

KINDS = ("first-ready", "post-ready")
FIRST_READY_DETAIL = "first boot has no BVAGENT READY/PONG evidence;"
SCHEMA = "bridgevm.t17-share-listing.v1"
ENTRY_CAP = 256
FILE_HASH_CAP = 64 * 1024 * 1024
TOTAL_HASH_CAP = 256 * 1024 * 1024
LISTING_CAP = 1024 * 1024
CHUNK = 1024 * 1024
DIR_FLAGS = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC
READ_FLAGS = os.O_RDONLY | os.O_NOFOLLOW | os.O_CLOEXEC | os.O_NONBLOCK
STATUSES = ("listed", "absent", "unsafe", "error")
FILE_REASONS = ("none", "over-cap", "unsafe", "changed", "error")
LISTING_KEYS = {"schema_version", "status", "reason", "entry_count", "truncated", "hashed_bytes", "entries"}
ENTRY_KEYS = {"name", "type", "bytes", "sha256", "reason"}
HEX = re.compile(r"[0-9a-f]{64}\Z")
REASON = re.compile(r"[A-Za-z0-9][A-Za-z0-9._-]{0,63}\Z")


def matches(kind: object, result: dict, detail: object, claims: list[str]) -> bool:
    """Whether an authenticated lane result is a failure of this packet kind."""
    first = kind == "first-ready"
    return (kind in KINDS and result.get("failure_code") == "guest-evidence-missing"
            and result.get("first_ready") is (not first) and isinstance(detail, str)
            and detail.startswith(FIRST_READY_DETAIL) == first
            and (len(claims) == 1 if first else len(claims) <= 1))


def host_stop(claims: list[str]) -> str:
    return claims[0] if claims else "not-requested"


def listing_name(ordinal: int) -> str:
    return f"t17-diagnostic-lane-{ordinal}-share-listing.json"


def entry_type(mode: int) -> str:
    kinds = ((stat.S_ISREG, "file"), (stat.S_ISDIR, "directory"), (stat.S_ISLNK, "symlink"))
    return next((name for test, name in kinds if test(mode)), "other")


def identity(info: os.stat_result) -> tuple[int, ...]:
    return (info.st_dev, info.st_ino, info.st_mode, info.st_nlink, info.st_size, info.st_mtime_ns)


def file_digest(share: int, name: str, before: os.stat_result, device: int) -> tuple[str | None, str]:
    if before.st_nlink != 1 or before.st_uid != os.geteuid() or before.st_dev != device:
        return None, "unsafe"
    fd = os.open(name, READ_FLAGS, dir_fd=share)
    try:
        if identity(os.fstat(fd)) != identity(before):
            return None, "changed"
        checksum, position = hashlib.sha256(), 0
        while position < before.st_size:
            block = os.pread(fd, min(CHUNK, before.st_size - position), position)
            if not block:
                return None, "changed"
            checksum.update(block)
            position += len(block)
        after = (identity(os.fstat(fd)), identity(os.stat(name, dir_fd=share, follow_symlinks=False)))
        if after != (identity(before),) * 2:
            return None, "changed"
        return checksum.hexdigest(), "none"
    finally:
        os.close(fd)


def listing(lane: int, device: int) -> dict:
    """Describe <lane>/share by name, type, size and digest only."""
    value = {"schema_version": SCHEMA, "status": "listed", "reason": "none", "entry_count": 0,
             "truncated": False, "hashed_bytes": 0, "entries": []}
    try:
        before = os.stat("share", dir_fd=lane, follow_symlinks=False)
    except FileNotFoundError:
        return {**value, "status": "absent"}
    if not stat.S_ISDIR(before.st_mode) or before.st_uid != os.geteuid() or before.st_dev != device:
        return {**value, "status": "unsafe", "reason": "share-directory"}
    try:
        share = os.open("share", DIR_FLAGS, dir_fd=lane)
        try:
            if identity(os.fstat(share)) != identity(before):
                return {**value, "status": "unsafe", "reason": "changed"}
            names = sorted(os.listdir(share))
            for name in names[:ENTRY_CAP]:
                entry = {"name": name, "type": "unknown", "bytes": None, "sha256": None, "reason": "error"}
                value["entries"].append(entry)
                try:
                    info = os.stat(name, dir_fd=share, follow_symlinks=False)
                    kind = entry["type"] = entry_type(info.st_mode)
                    entry.update(bytes=info.st_size if kind == "file" else None, reason="over-cap" if kind == "file" else "not-regular")
                    if kind == "file" and info.st_size <= FILE_HASH_CAP and value["hashed_bytes"] + info.st_size <= TOTAL_HASH_CAP:
                        entry["sha256"], entry["reason"] = file_digest(share, name, info, device)
                        value["hashed_bytes"] += info.st_size if entry["sha256"] else 0
                except OSError:
                    entry["reason"] = "error"
        finally:
            os.close(share)
    except OSError as error:
        return {**value, "status": "error", "reason": errno.errorcode.get(error.errno, "unknown"),
                "hashed_bytes": 0, "entries": []}
    return {**value, "entry_count": len(names), "truncated": len(names) > ENTRY_CAP}


def encode(value: dict) -> bytes:
    body = (json.dumps(value, sort_keys=True, indent=2) + "\n").encode("utf-8")
    if len(body) > LISTING_CAP:
        raise ValueError("share listing exceeds its cap")
    return body


def natural(value: object) -> bool:
    return type(value) is int and value >= 0


def verify(value: object) -> None:
    """Independently re-check a retained share listing's schema and totals."""
    if (not isinstance(value, dict) or set(value) != LISTING_KEYS or value["schema_version"] != SCHEMA
            or value["status"] not in STATUSES or not isinstance(value["reason"], str)
            or not REASON.fullmatch(value["reason"]) or (value["status"] in ("listed", "absent")) != (value["reason"] == "none")
            or not natural(value["entry_count"]) or not natural(value["hashed_bytes"])
            or type(value["truncated"]) is not bool or not isinstance(value["entries"], list)
            or len(value["entries"]) != min(value["entry_count"], ENTRY_CAP)
            or value["truncated"] != (value["entry_count"] > ENTRY_CAP)
            or value["status"] != "listed" and value["entry_count"] != 0):
        raise ValueError("share listing schema differs")
    names, hashed = [], 0
    for entry in value["entries"]:
        if (not isinstance(entry, dict) or set(entry) != ENTRY_KEYS or not isinstance(entry["name"], str)
                or not 1 <= len(entry["name"].encode("utf-8", "surrogateescape")) <= 255
                or "/" in entry["name"] or "\0" in entry["name"] or entry["name"] in (".", "..")
                or entry["type"] not in ("file", "directory", "symlink", "other", "unknown")):
            raise ValueError("share listing entry differs")
        if entry["type"] != "file":
            if (entry["bytes"], entry["sha256"], entry["reason"]) != (None, None, "error" if entry["type"] == "unknown" else "not-regular"):
                raise ValueError("share listing non-file entry claims bytes")
        elif (not natural(entry["bytes"]) or entry["reason"] not in FILE_REASONS
              or (entry["reason"] == "none") != (isinstance(entry["sha256"], str) and bool(HEX.fullmatch(entry["sha256"])))
              or entry["reason"] != "none" and entry["sha256"] is not None
              or entry["reason"] == "none" and entry["bytes"] > FILE_HASH_CAP):
            raise ValueError("share listing file entry differs")
        hashed += entry["bytes"] if entry["reason"] == "none" else 0
        names.append(entry["name"])
    if names != sorted(set(names)) or hashed != value["hashed_bytes"] or hashed > TOTAL_HASH_CAP:
        raise ValueError("share listing order or hashed total differs")
