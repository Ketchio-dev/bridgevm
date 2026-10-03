"""Authenticate retained development observations before releasing queue ownership."""
import hashlib
import json
import os
from pathlib import Path
import stat
import re

from native_snapshot_restore_public import _unique_fields, _reject_constant
from native_snapshot_restore_seal import read_bounded_regular
from t22_pair_admission import read_query, shutdown_observed
from t22_pair_provenance import SHA, file_hash
from t22_pair_queue_inputs import stable_output, SCRIPT


def document(path, limit=65_536):
    raw = read_bounded_regular(path, limit, readonly=True)
    info = path.lstat()
    if info.st_nlink != 1 or info.st_uid != os.geteuid(): raise ValueError("unsafe development proof file")
    value = json.loads(raw, object_pairs_hook=_unique_fields, parse_constant=_reject_constant)
    if type(value) is not dict: raise ValueError("development proof must be an object")
    return value, hashlib.sha256(raw).hexdigest()


def immutable_tree(root):
    count = 0; device = Path.home().stat().st_dev
    for directory, subdirs, files in os.walk(root, followlinks=False):
        for path in (Path(directory), *(Path(directory) / n for n in subdirs + files)):
            info = path.lstat(); count += 1
            if (count > 1024 or info.st_dev != device or info.st_uid != os.geteuid() or info.st_mode & 0o222
                    or not (stat.S_ISDIR(info.st_mode) or stat.S_ISREG(info.st_mode))
                    or (stat.S_ISREG(info.st_mode) and info.st_nlink != 1)):
                raise ValueError("preparation output is mutable, aliased or unsafe")


def gone(pid, group=False):
    if type(pid) is not int or not 2 <= pid < 2 ** 31: raise ValueError("invalid owned process identity")
    try:
        (os.killpg if group else os.kill)(pid, 0)
    except ProcessLookupError:
        return
    raise ValueError("owned process is present or cannot be queried")


def context(root, core, identity):
    work = root / "live"; info = work.lstat()
    base = {"commit": identity["commit"], "job_id": identity["job_id"],
            "work_device": info.st_dev, "work_inode": info.st_ino}
    attempt, digest = document(work / "owned-launch-attempt.json")
    if attempt != {"schema": "bridgevm.t22-launch-attempt.v1", **base} or any(
            type(attempt[k]) is not type(v) for k, v in base.items()):
        raise ValueError("launch attempt differs from owned preparation")
    observed, hashed = document(work / "owned-launch-context.json")
    expected = {"schema": "bridgevm.t22-owned-launch.v1", **base, "attempt_sha256": digest,
                "pid": observed.get("pid"), "group_verified": True,
                "terminal_observed": True, "group_absence_observed": True}
    if set(observed) != set(expected) or any(type(observed[k]) is not type(v) or observed[k] != v
                                           for k, v in expected.items()):
        raise ValueError("launch ownership or terminal observation is unproved")
    if (core.get("owned_launch_attempt_sha256") != digest
            or core.get("owned_launch_context_sha256") != hashed):
        raise ValueError("launch context hash differs from core receipt")
    gone(observed["pid"], group=True)
    return hashed


def adapter_context(directory, identity, expected_command):
    attempt, digest = document(directory / "adapter-launch-attempt.json")
    value, hashed = document(directory / "adapter-launch-context.json")
    base = {"commit": identity["commit"], "job_id": identity["job_id"]}
    if (set(attempt) != {"schema", *base, "command_sha256"}
            or attempt["schema"] != "bridgevm.t22-adapter-attempt.v1"
            or any(type(attempt[k]) is not str or attempt[k] != v for k, v in base.items())
            or type(attempt["command_sha256"]) is not str or not SHA.fullmatch(attempt["command_sha256"])):
        raise ValueError("adapter attempt differs from job")
    keys = {"schema", *base, "command_sha256", "attempt_sha256", "core_pid", "terminal_observed", "exit_code", "cause"}
    if (set(value) != keys or value["schema"] != "bridgevm.t22-adapter-launch.v1"
            or any(type(value[k]) is not str or value[k] != v for k, v in base.items())
            or value["command_sha256"] != attempt["command_sha256"] or value["command_sha256"] != expected_command
            or value["attempt_sha256"] != digest
            or value["terminal_observed"] is not True or type(value["exit_code"]) is not int
            or not -255 <= value["exit_code"] <= 255
            or value["cause"] not in ("completed", "timeout", "canceled", "interrupted")):
        raise ValueError("adapter terminal ownership is unproved")
    gone(value["core_pid"])
    return value, hashed


def core_proof(directory, identity, native_hash):
    output = stable_output(identity["job_id"])
    if output.is_symlink() or output.resolve() != output: raise ValueError("unsafe stable preparation root")
    immutable_tree(output)
    if os.path.lexists(output / "cleanup-required.env"): raise ValueError("standalone preparation requires cleanup")
    core, hashed = document(output / "preparation-receipt.json")
    fixed = {"schema": "bridgevm.t22-owned-pair-preparation.v1", "commit": identity["commit"],
             "job_id": identity["job_id"], "purpose": "development-only", "claim_eligible": False,
             "criterion_pass": False, "future_tpm_independence_proven": False,
             "input_manifest_sha256": native_hash, "origin_manifest_sha256": identity["origin_manifest_sha256"],
             "query_script_sha256": identity["query_script_sha256"], "cleanup_complete": True,
             "cleanup_required": False, "source_integrity": True, "vtpm_configured": False,
             "initial_boot_writes_owned_clones": True}
    flags = {"preparation_complete", "complete", "natural_shutdown_observed", "encryption_observed_after_initial_boot"}
    optional = {"partition_count", "ntfs_partition_count", "firmware_sha256", "boot_config_sha256", "output_hashes",
                "failure_type", "cleanup_failure_type", "ownership_uncertain", "owned_launch_attempt_sha256",
                "owned_launch_context_sha256", "query_result_sha256", "shutdown_log_sha256", "t22_input_manifest_sha256",
                "fixed_volume_count", "decrypted_ntfs_volume_count"}
    if not set(core).issubset(set(fixed) | flags | optional): raise ValueError("unexpected core proof fields")
    for key in set(core) - set(fixed) - flags:
        if key.endswith("sha256") and (type(core[key]) is not str or not SHA.fullmatch(core[key])):
            raise ValueError("invalid core hash field")
        if key.endswith("_count") and (type(core[key]) is not int or not 1 <= core[key] <= 128):
            raise ValueError("invalid core count")
        if key.endswith("_type") and (type(core[key]) is not str or not re.fullmatch(r"[A-Za-z][A-Za-z0-9]{0,63}", core[key])):
            raise ValueError("invalid core failure label")
    if "ownership_uncertain" in core: raise ValueError("core ownership remains uncertain")
    if any(type(core.get(k)) is not type(v) or core[k] != v for k, v in fixed.items()):
        raise ValueError("core preparation identity, source or cleanup is unproved")
    for key in ("preparation_complete", "complete", "natural_shutdown_observed", "encryption_observed_after_initial_boot"):
        if type(core.get(key)) is not bool: raise ValueError("invalid core preparation flag")
    if core["preparation_complete"] != core["complete"]: raise ValueError("core completion flags differ")
    ctx = context(output, core, identity)
    share = output / "live/share"; results = list(share.glob("result-*.json"))
    if len(results) != 1 or len(list(share.glob("result-*.done"))) != 1 or list(share.glob("result-*.pending")):
        raise ValueError("missing, partial or duplicated query result")
    query, _ = document(results[0])
    nonce = query.get("nonce")
    if type(nonce) is not str or results[0].name != f"result-{nonce}.json": raise ValueError("query filename identity differs")
    facts, digest = read_query(results[0].parent, nonce, identity["query_script_sha256"])
    if (core.get("query_result_sha256") != digest or any(type(core.get(k)) is not int or core[k] != v for k, v in facts.items())
            or file_hash(results[0].parent / Path(SCRIPT).name, readonly=True) != identity["query_script_sha256"]):
        raise ValueError("core query fields differ from retained raw evidence")
    if core["encryption_observed_after_initial_boot"] is not True: raise ValueError("query observation was not retained")
    clones = {k: output / "live" / f for k, f in (("image", "image.raw"), ("vars", "vars.fd"))}
    hashes = {k: file_hash(p, readonly=True) for k, p in clones.items()}
    if core.get("output_hashes") != hashes: raise ValueError("prepared clone hashes differ")
    manifest = output / "t22-input-manifest.tsv"
    manifest_hash = "absent"
    if core["preparation_complete"]:
        if core["natural_shutdown_observed"] is not True or "failure_type" in core: raise ValueError("unsupported core completion")
        if shutdown_observed(0, output / "live/boot/run.log") != core.get("shutdown_log_sha256"):
            raise ValueError("shutdown evidence differs from core")
        manifest_hash = file_hash(manifest, readonly=True)
        if manifest_hash != core.get("t22_input_manifest_sha256"): raise ValueError("final prepared manifest differs")
    elif os.path.lexists(manifest): raise ValueError("failed core carries a prepared manifest")
    return core, hashed, ctx, hashes, manifest_hash, facts
