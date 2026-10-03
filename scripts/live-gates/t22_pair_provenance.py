"""Admit a stopped retained pair without opening any vTPM or key artifact."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import stat

from native_snapshot_restore_inputs import parse_manifest, authenticate
from native_snapshot_restore_public import _reject_constant, _unique_fields
from native_snapshot_restore_seal import read_bounded_regular
from product_e2e_identity import fixed_fields_match
from product_e2e_json_snapshot import JsonSnapshot

SHA = re.compile(r"[0-9a-f]{64}\Z")
COMMIT = re.compile(r"[0-9a-f]{40}\Z")
HERE = Path(__file__).resolve().parent


def file_hash(path, readonly=False):
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    try:
        before = os.fstat(fd)
        if (not stat.S_ISREG(before.st_mode) or before.st_nlink != 1
                or before.st_size == 0 or (readonly and before.st_mode & 0o222)):
            raise ValueError("unsafe retained pair file")
        value = hashlib.sha256()
        while block := os.read(fd, 1024 * 1024):
            value.update(block)
        after = os.fstat(fd)
    finally:
        os.close(fd)
    current = os.stat(path, follow_symlinks=False)
    def identity(info):
        return (info.st_dev, info.st_ino, info.st_mode, info.st_nlink,
                info.st_size, info.st_mtime_ns, info.st_ctime_ns)
    if identity(before) != identity(after) or identity(after) != identity(current):
        raise ValueError("retained pair changed during hashing")
    return value.hexdigest()


def snapshot(path, expected):
    document = JsonSnapshot.read(path)
    document.require_seal(expected)
    # JsonSnapshot rejects duplicates; additionally reject nonfinite values.
    json.loads(document.data, object_pairs_hook=_unique_fields, parse_constant=_reject_constant)
    return document


def document_entry(entry):
    if type(entry) is not dict or set(entry) != {"path", "sha256"}:
        raise ValueError("invalid lineage document")
    path = Path(entry["path"]) if type(entry["path"]) is str else Path(".")
    if not path.is_absolute() or str(path.resolve()) != str(path):
        raise ValueError("lineage path must be canonical and absolute")
    return snapshot(path, entry["sha256"])


def load_writer(tier):
    filename = {"T17": "write-windows-product-e2e-receipt.py",
                "T19": "write-windows-import-product-e2e-receipt.py"}.get(tier)
    if filename is None:
        raise ValueError("unsupported retained origin")
    spec = importlib.util.spec_from_file_location("t22_origin_" + tier, HERE / filename)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def origin(path, expected, rows):
    document = snapshot(path, expected)
    value = document.value
    keys = {"schema", "tier", "job_id", "commit", "mode", "lane",
            "request", "result", "stamp", "selected_manifest"}
    if (type(value) is not dict or set(value) != keys
            or value["schema"] != "bridgevm.t22-retained-origin.v1"
            or value["tier"] not in ("T17", "T19")
            or type(value["commit"]) is not str or not COMMIT.fullmatch(value["commit"])
            or type(value["job_id"]) is not str or not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]{0,127}", value["job_id"])
            or value["mode"] not in ("pilot", "release") or type(value["lane"]) is not int
            or not 1 <= value["lane"] <= (3 if value["mode"] == "release" else 1)):
        raise ValueError("invalid retained origin identity")
    request, result, stamp, selected = (document_entry(value[key]) for key in
                                       ("request", "result", "stamp", "selected_manifest"))
    writer = load_writer(value["tier"])
    if value["tier"] == "T17":
        lane = writer.lane(result.path, job_id=value["job_id"], commit=value["commit"],
                           mode=value["mode"], ordinal=value["lane"], stamp=stamp.path, _document=result)
    else:
        lane = writer.lane(result.path, value["job_id"], value["commit"], value["mode"],
                           value["lane"], stamp.path, _document=result)
    if (lane["failure_code"] != "none" or lane["cleanup_verified"] is not True
            or lane["ui_frontend_automated"] is not True or not all(lane[k] is True for k in writer.LANE_STAGES)):
        raise ValueError("retained lane is incomplete")
    prefix = lane["nonce"][:12]
    kind = "windows-hvf-3d-off" if value["tier"] == "T17" else "windows-hvf-import"
    title = "BridgeVM T17 Lane" if value["tier"] == "T17" else "BridgeVM A9 Import Lane"
    slug = "bridgevm-t17-lane" if value["tier"] == "T17" else "bridgevm-a9-import-lane"
    fixed = {"schema_version": f"bridgevm.{kind}-product-e2e-request." + ("v2" if value["tier"] == "T17" else "v1"),
             "job_id": value["job_id"], "commit": value["commit"], "campaign_mode": value["mode"],
             "lane": value["lane"], "nonce": lane["nonce"], "three_d_injection": False,
             "vm_name": f"{title} {value['lane']} {prefix}", "vm_slug": f"{slug}-{value['lane']}-{prefix}"}
    if (type(request.value) is not dict or set(request.value) != writer.REQUEST_KEYS
            or not fixed_fields_match(request.value, fixed)
            or any(type(request.value[k]) is not str or not request.value[k].startswith("/") for k in writer.REQUEST_PATHS)
            or stamp.value["request_sha256"] != request.sha256):
        raise ValueError("retained request is not bound to its stamp")
    pair = selected.value
    directory = selected.path.parent
    if (selected.path.name != "manifest.json" or directory.is_symlink()
            or directory.stat().st_mode & 0o222
            or selected.path.stat().st_mode & 0o222 or selected.path.stat().st_nlink != 1
            or {p.name for p in directory.iterdir()} != {"disk.raw", "vars.fd", "manifest.json"}
            or Path(rows["vars"][0]).stat().st_size != 64 << 20
            or Path(rows["image"][0]).stat().st_size % 512):
        raise ValueError("retained pair layout or geometry is unsafe")
    expected_pair = {"format_version": 1, "vm_id": fixed["vm_slug"],
                     "disk_bytes": Path(rows["image"][0]).stat().st_size,
                     "disk_sha256": lane["final_disk_sha256"], "vars_bytes": 64 << 20,
                     "vars_sha256": lane["final_vars_sha256"]}
    if (type(pair) is not dict or set(pair) != set(expected_pair)
            or not fixed_fields_match(pair, expected_pair)):
        raise ValueError("selected snapshot differs from authenticated final pair")
    for name, filename, field in (("image", "disk.raw", "disk_sha256"), ("vars", "vars.fd", "vars_sha256")):
        media = Path(rows[name][0])
        if (media != selected.path.parent / filename or file_hash(media, readonly=True) != pair[field]
                or rows[name][1] != pair[field]):
            raise ValueError("input is not the immutable selected pair")
    if os.path.samefile(rows["image"][0], rows["vars"][0]):
        raise ValueError("retained disk and vars are aliased")
    documents = (document, request, result, stamp, selected)
    for item in documents:
        item.require_unchanged()
    return documents


def admit(manifest, manifest_sha, origin_path, origin_sha, commit):
    data = read_bounded_regular(manifest, 65_536)
    if not SHA.fullmatch(manifest_sha) or hashlib.sha256(data).hexdigest() != manifest_sha:
        raise ValueError("T22 manifest differs from its pinned hash")
    rows = parse_manifest(data, commit)
    for name in ("image", "vars"):
        path = Path(rows[name][0])
        if str(path.resolve()) != str(path) or path.is_symlink():
            raise ValueError("pair paths must be canonical")
    authenticate(rows, Path(rows["binary"][0]))
    documents = origin(origin_path, origin_sha, rows)
    return rows, data, documents


def unchanged(manifest, data, documents, rows):
    if read_bounded_regular(manifest, 65_536) != data:
        raise ValueError("T22 input manifest changed")
    for document in documents:
        document.require_unchanged()
    authenticate(rows, Path(rows["binary"][0]))
    for name in ("image", "vars"):
        if file_hash(Path(rows[name][0]), readonly=True) != rows[name][1]:
            raise ValueError("retained source changed")
