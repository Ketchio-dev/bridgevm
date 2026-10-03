"""Exact nonpromoting public/private D10 queue receipts, authenticated at publication."""
import hashlib
import os
from pathlib import Path

from native_snapshot_restore_inputs import FILES, METADATA, parse_manifest, authenticate
from native_snapshot_restore_seal import read_bounded_regular
from t22_pair_provenance import SHA
from t22_pair_queue_inputs import TIER, job, bound, stable_output
from t22_pair_queue_proof import document, core_proof, adapter_context
from t22_pair_queue_command import command, command_hash, store

SCHEMA = "bridgevm.t22-pair-queue.v1"
HASHES = {"preparation_receipt_sha256", "owned_launch_context_sha256", "adapter_context_sha256",
          "prepared_manifest_sha256", "prepared_disk_sha256", "prepared_vars_sha256"}
COUNTS = {"fixed_volume_count", "decrypted_ntfs_volume_count"}


def empty(identity, reason):
    value = {"schema": SCHEMA, **identity, "reason": reason, "preparation_complete": False,
             "worker_cleanup_verified": False, "pass": False, "claim_eligible": False, "criterion_pass": False}
    value.update({k: "absent" for k in HASHES}); value.update({k: 0 for k in COUNTS})
    return value


def checked(value, identity):
    fixed = empty(identity, "incomplete")
    if type(value) is not dict or set(value) != set(fixed): raise ValueError("unexpected development receipt fields")
    for key, expected in fixed.items():
        item = value[key]
        if key in HASHES:
            if type(item) is not str or (item != "absent" and not SHA.fullmatch(item)): raise ValueError("invalid development proof hash")
        elif key in COUNTS:
            if type(item) is not int or not 0 <= item <= 16: raise ValueError("invalid development proof count")
        elif key in ("preparation_complete", "worker_cleanup_verified"):
            if type(item) is not bool: raise ValueError("invalid development boolean")
        elif key == "reason":
            if item not in ("prepared", "clean-refusal", "incomplete", "canceled", "invalid-receipt"):
                raise ValueError("invalid development result reason")
        elif type(item) is not type(expected) or item != expected: raise ValueError("development receipt identity or claim differs")
    if value["preparation_complete"] and (value["reason"] != "prepared" or value["worker_cleanup_verified"] is not True
            or any(value[k] == "absent" for k in HASHES) or any(value[k] < 1 for k in COUNTS)):
        raise ValueError("unproved preparation result")
    if not value["preparation_complete"] and any(value[k] != 0 for k in COUNTS): raise ValueError("refusal carries success counts")
    return value


def collect(directory, root, commit):
    identity, rows, native, docs = bound(directory, root, commit)
    digest = hashlib.sha256(native).hexdigest()
    launcher, launcher_hash = adapter_context(directory, identity, command_hash(command(directory, root, identity, digest)))
    core, core_hash, ctx, hashes, manifest_hash, facts = core_proof(directory, identity, digest)
    output = stable_output(identity["job_id"])
    if (output / "live/vars.fd").stat().st_size != 64 << 20 or (output / "live/image.raw").stat().st_size != Path(rows["image"][0]).stat().st_size:
        raise ValueError("prepared pair geometry differs")
    if core["preparation_complete"]:
        candidate = parse_manifest(read_bounded_regular(output / "t22-input-manifest.tsv", 65_536, readonly=True), commit)
        for key in FILES | METADATA:
            expected = [str(output / "live" / ("image.raw" if key == "image" else "vars.fd")), hashes[key]] if key in hashes else rows[key]
            if candidate[key] != expected: raise ValueError("prepared manifest differs from source or owned pair")
        authenticate(candidate, directory / "hvf_gic_boot_probe")
    prepared = core["preparation_complete"] and launcher["cause"] == "completed" and launcher["exit_code"] == 0 and not (directory / "cancel.requested").exists()
    reason = "prepared" if prepared else ("canceled" if (directory / "cancel.requested").exists() else "clean-refusal")
    value = empty(identity, reason)
    value.update(preparation_complete=prepared, worker_cleanup_verified=True,
                 preparation_receipt_sha256=core_hash, owned_launch_context_sha256=ctx,
                 adapter_context_sha256=launcher_hash, prepared_manifest_sha256=manifest_hash,
                 prepared_disk_sha256=hashes["image"], prepared_vars_sha256=hashes["vars"])
    if prepared: value.update(facts)
    for item in docs: item.require_unchanged()
    return checked(value, identity)


def finalize(directory, root, commit):
    identity = job(directory, commit); path = directory / "receipt.json"
    if os.path.lexists(path):
        try:
            value, _ = document(path); checked(value, identity)
            if not (directory / "cancel.requested").exists(): return
            reason = "canceled"
        except (OSError, ValueError): reason = "invalid-receipt"
        target = directory / "receipt.before-finalize.json"
        if os.path.lexists(target): raise ValueError("refusing to overwrite prior development receipt")
        os.rename(path, target)
        if reason == "invalid-receipt": store(path, empty(identity, reason)); return
    try: value = collect(directory, root, commit)
    except (OSError, ValueError): value = empty(identity, "canceled" if (directory / "cancel.requested").exists() else "incomplete")
    store(path, value)


def publish(directory, root, commit):
    value, _ = document(directory / "receipt.json")
    if value != collect(directory, root, commit): raise ValueError("development receipt differs from retained proofs")
    store(directory / "receipt.public.json", value)


def guard(directory, root, commit, job_id):
    identity = job(directory, commit, job_id)
    value, _ = document(directory / "receipt.json"); checked(value, identity)
    if value["worker_cleanup_verified"] is not True or value != collect(directory, root, commit):
        raise ValueError("development cleanup remains unproved")
