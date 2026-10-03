"""Publish only after clone writers ended and every source/probe still matches."""
import hashlib
import json
import os
from pathlib import Path

from native_snapshot_restore_inputs import FILES, METADATA, parse_manifest, authenticate
from retained_windows_identity import directory_identity
from t22_pair_provenance import file_hash
from t22_pair_output_lock import seal_tree, publish_pending


def write_exclusive(path, data, dir_fd=None):
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600, dir_fd=dir_fd)
    with os.fdopen(fd, "wb") as stream:
        stream.write(data); stream.flush(); os.fsync(stream.fileno())


def publish(rows, clones, output, identity, receipt, commit):
    if (any(receipt.get(k) is not True for k in ("preparation_complete", "cleanup_complete",
             "source_integrity", "natural_shutdown_observed", "encryption_observed_after_initial_boot"))
            or receipt.get("vtpm_configured") is not False or receipt.get("claim_eligible") is not False
            or receipt.get("criterion_pass") is not False):
        raise ValueError("unproven pair preparation refuses publication")
    if directory_identity(output) != identity or any(not clones.get(k) for k in ("image", "vars")):
        raise ValueError("owned preparation directory changed")
    candidate = {key: list(value) for key, value in rows.items()}
    for name in ("image", "vars"):
        path = clones[name]
        if path != output / "live" / ("image.raw" if name == "image" else "vars.fd"):
            raise ValueError("clone publication path escaped its owned directory")
        digest = file_hash(path, readonly=True)
        if receipt.get("output_hashes", {}).get(name) != digest:
            raise ValueError("prepared clone changed before publication")
        candidate[name] = [str(path), digest]
    data = "".join("\t".join((key, *candidate[key])) + "\n" for key in sorted(FILES | METADATA)).encode()
    checked = parse_manifest(data, commit)
    authenticate(checked, Path(checked["binary"][0]))
    pending = output / "t22-input-manifest.pending"
    root = os.open(output, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    try:
        info = os.fstat(root)
        if (info.st_dev, info.st_ino) != identity:
            raise ValueError("output changed before pending manifest write")
        write_exclusive(pending.name, data, dir_fd=root)
    finally:
        os.close(root)
    if directory_identity(output) != identity:
        raise ValueError("owned preparation directory changed before publication")
    receipt["t22_input_manifest_sha256"] = hashlib.sha256(data).hexdigest()
    receipt["output_hashes"] = {k: candidate[k][1] for k in ("image", "vars")}


def record(output, identity, receipt):
    if directory_identity(output) != identity:
        raise ValueError("refusing to record into a replaced preparation directory")
    root = os.open(output, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    try:
        info = os.fstat(root)
        if (info.st_dev, info.st_ino) != identity:
            raise ValueError("output changed before receipt write")
        receipt["cleanup_required"] = receipt.get("cleanup_complete") is not True
        if receipt["cleanup_required"]:
            write_exclusive("cleanup-required.env", b"schema=bridgevm.t22-owned-pair-cleanup.v1\ncleanup_complete=false\n", dir_fd=root)
        write_exclusive("preparation-receipt.json", (json.dumps(receipt, sort_keys=True) + "\n").encode(), dir_fd=root)
    finally:
        os.close(root)
    if receipt.get("cleanup_complete") is True:
        with seal_tree(output, identity) as root:
            if receipt.get("preparation_complete") is True:
                if file_hash(output / "t22-input-manifest.pending", readonly=True) != receipt.get("t22_input_manifest_sha256"):
                    raise ValueError("pending manifest changed before publication")
                publish_pending(root)
