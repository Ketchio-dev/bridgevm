"""Strict retained helper outputs and owned staged-read evidence for T22."""
from __future__ import annotations

import hashlib
from pathlib import Path
import re

from a19_first_restore_collection import parse_stop
from a19_interrupt_restore_child import read_fd_observed
from a19_interrupt_stop_points import stable_root
from a19_interrupted_restore_seal import read_bounded_regular
from native_snapshot_export_json import load_json

PAIR = ("disk_bytes", "disk_sha256", "vars_bytes", "vars_sha256")
MANIFEST = ("format_version", "vm_id", *PAIR)
SHA256 = re.compile(r"[0-9a-f]{64}\Z")
SIZE = re.compile(r"[1-9][0-9]{0,19}\Z")


def media(value: dict) -> dict:
    result = {field: value[field] for field in PAIR}
    for field in PAIR:
        item = result[field]
        if field.endswith("_bytes"):
            if type(item) is not int or not 1 <= item <= 2**64 - 1:
                raise ValueError("auxiliary media size is invalid")
        elif type(item) is not str or not SHA256.fullmatch(item):
            raise ValueError("auxiliary media hash is invalid")
    return result


def manifest(value: object, vm_id: str) -> dict:
    if (not isinstance(value, dict) or set(value) != set(MANIFEST) or
            type(value["format_version"]) is not int or value["format_version"] != 1 or
            value["vm_id"] != vm_id):
        raise ValueError("auxiliary retry manifest identity is invalid")
    media(value)
    return value


def protocol(path: Path, vm_id: str | None = None) -> tuple[dict, str]:
    raw = read_bounded_regular(path, 4096)
    pairs = [line.split(" ", 1) for line in raw.decode("ascii").splitlines()]
    fields = MANIFEST if vm_id is not None else PAIR
    if any(len(pair) != 2 for pair in pairs) or [pair[0] for pair in pairs] != list(fields):
        raise ValueError("auxiliary helper output has invalid fields")
    value = dict(pairs)
    for field in (*[key for key in fields if key.endswith("_bytes")], "format_version"):
        if field not in value:
            continue
        if not SIZE.fullmatch(value[field]):
            raise ValueError("auxiliary helper output has invalid integers")
        value[field] = int(value[field])
    if vm_id is not None:
        manifest(value, vm_id)
    else:
        media(value)
    return value, hashlib.sha256(raw).hexdigest()


def selected(root: Path, name: str) -> dict:
    helper, _ = protocol(root / f"{name}-digest.txt")
    host, _ = protocol(root / f"{name}-host-digest.txt")
    if helper != host:
        raise ValueError("auxiliary helper and independent host hashes disagree")
    return helper


def stop(root: Path, point: str, output: Path, case: str) -> dict:
    value = parse_stop(root / "interrupt-observation.json",
                       root / "interrupt-helper-fd.private.log", point)
    context, _ = load_json(root / "interrupt-helper-context.private.json")
    if case == "first":
        bundle = output.resolve() / "live/library/a19-native-cli-live/bundle.vmbridge"
        expected = stable_root(bundle / "disks/hvf-target.raw", bundle / "metadata/hvf-vars.fd") / "staging/disk.raw"
    else:
        work = output.resolve() / "live/auxiliary" / case
        expected = (stable_root(work / "disk.raw", work / "vars.fd") / "staging/disk.raw"
                    if case == "swap" else work / ".export.snapshot.staging/disk.raw")
    if (not isinstance(context, dict) or set(context) != {
            "helper_pid", "staged_disk_path", "selection_before", "selection_after"} or
            type(context["helper_pid"]) is not int or context["helper_pid"] <= 1 or
            context["staged_disk_path"] != str(expected)):
        raise ValueError("stop context does not bind the owned staged disk")
    selected_before = context["selection_before"]
    if (context["selection_after"] != selected_before or
            (case == "first" and selected_before != "initial") or
            (case == "create" and selected_before != "absent") or
            (case == "swap" and (type(selected_before) is not str or not SHA256.fullmatch(selected_before)))):
        raise ValueError("stop context does not prove the required unchanged selection")
    raw = read_bounded_regular(root / "interrupt-helper-fd.private.log", 65_536)
    if (value["stop_fd_log_sha256"] != hashlib.sha256(raw).hexdigest() or
            not read_fd_observed(raw.decode("utf-8"), context["helper_pid"], expected)):
        raise ValueError("auxiliary stop log does not prove an owned read FD")
    return value
