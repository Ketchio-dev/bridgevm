"""Allowlisted, nonpromoting D11 receipts with independent cleanup checks."""
import os
from pathlib import Path

from d11_fixture_files import canonical, digest, document, identity, record
from d11_fixture_inputs import SHA
from d11_fixture_mounts import Container, inventory
from d11_fixture_queue_inputs import bound, output_path, job
from guest_input_owned_group import state

HASHES = {"preparation_sha256", "disk_sha256", "vars_sha256", "container_sha256", "guest_result_sha256", "source_sha256"}
STAGES = {"installed", "ready_stopped", "sealed_fixture"}
REASONS = {"prepared", "clean-refusal", "incomplete", "canceled", "cleanup-unproved"}


def empty(binding, reason):
    return {"schema": "bridgevm.d11-fixture-queue.v1", **binding, "classification": "DEVELOPMENT_ONLY",
            "reason": reason, "pass": False, "claim_eligible": False, "criterion_pass": False,
            "t15_ready": False, "worker_cleanup_verified": False,
            **{k: False for k in STAGES}, **{k: "absent" for k in HASHES}}


def checked(value, binding):
    template = empty(binding, "incomplete")
    if type(value) is not dict or set(value) != set(template): raise ValueError("unexpected fixture receipt fields")
    for key, expected in template.items():
        item = value[key]
        if key in HASHES:
            if type(item) is not str or (item != "absent" and not SHA.fullmatch(item)):
                raise ValueError("invalid fixture proof hash")
        elif key in STAGES | {"worker_cleanup_verified"}:
            if type(item) is not bool: raise ValueError("fixture stage must be boolean")
        elif key == "reason":
            if item not in REASONS: raise ValueError("unknown fixture reason")
        elif type(item) is not type(expected) or item != expected:
            raise ValueError("fixture identity or claim differs")
    if value["ready_stopped"] and not value["installed"]: raise ValueError("fixture stages out of order")
    if value["sealed_fixture"] and (not value["ready_stopped"] or not value["worker_cleanup_verified"]
            or value["reason"] != "prepared" or any(value[k] == "absent" for k in HASHES)):
        raise ValueError("unproved sealed fixture")
    if (value["reason"] == "prepared") != value["sealed_fixture"]:
        raise ValueError("fixture completion differs")
    return value


def collect(directory, commit, current=True):
    binding, rows = bound(directory, commit)
    value = empty(binding, "incomplete")
    if (directory / "d11-refusal.private.json").exists(): return empty(binding, "cleanup-unproved")
    output = output_path(binding["job_id"])
    if not os.path.lexists(output):
        value.update(reason="canceled" if (directory / "cancel.requested").exists() else "clean-refusal",
                     worker_cleanup_verified=True)
        return checked(value, binding)
    canonical(output)
    private = output / "preparation.private.json"
    core = document(private)
    if (core.get("schema") != "bridgevm.d11-fixture-private.v1"
            or core.get("job_id") != binding["job_id"] or core.get("commit") != commit
            or core.get("input_manifest_sha256") != binding["input_manifest_sha256"]
            or core.get("binary_sha256") != binding["sealed_binary_sha256"]
            or core.get("output_identity") != identity(output.lstat())[:2]
            or core.get("t15_ready") is not False or type(core.get("processes")) is not list):
        raise ValueError("fixture private proof differs")
    clean = core.get("cleanup_verified") is True
    for row in core["processes"]:
        if (type(row) is not dict or set(row) != {"pid", "terminal_observed", "exit_code", "group_absent"}
                or type(row["pid"]) is not int or row["pid"] <= 1
                or row["terminal_observed"] is not True or type(row["exit_code"]) is not int
                or row["group_absent"] is not True):
            clean = False
        elif current and state(row["pid"]) != "absent": clean = False
    if current:
        mounts = Container(output, Path(rows["iso"][0]), None, int(rows["container_gib"][0]))
        if any(mounts.belongs(row) for row in inventory()): clean = False
    value.update({k: core.get(k, False) for k in STAGES})
    value.update({k: core.get(k, "absent") for k in HASHES - {"preparation_sha256"}})
    value.update(preparation_sha256=digest(private), worker_cleanup_verified=clean)
    if current and core.get("sealed_fixture") is True:
        backing = output / "fixture.sparseimage"
        if (core.get("backing_identity") != identity(backing.lstat()) or backing.stat().st_mode & 0o222
                or backing.is_symlink()): raise ValueError("sealed fixture backing changed")
    reason = "prepared" if core.get("sealed_fixture") is True else core.get("failure", "incomplete")
    if reason == "none": reason = "incomplete"
    if not clean: reason = "cleanup-unproved"
    if (directory / "cancel.requested").exists(): reason = "canceled"
    value["reason"] = reason
    if reason != "prepared": value["sealed_fixture"] = False
    return checked(value, binding)


def finalize(directory, commit):
    binding, _ = bound(directory, commit)
    try: value = collect(directory, commit)
    except (OSError, ValueError): value = empty(binding, "cleanup-unproved")
    target = directory / "receipt.json"
    if os.path.lexists(target):
        if document(target) == value: return
        if os.path.lexists(directory / "receipt.before-finalize.private.json"):
            raise ValueError("fixture finalization already retained a prior receipt")
        os.rename(target, directory / "receipt.before-finalize.private.json")
    record(target, value)


def publish(directory, commit):
    binding, _ = bound(directory, commit)
    value = checked(document(directory / "receipt.json"), binding)
    if value != collect(directory, commit): raise ValueError("fixture publication differs from proofs")
    record(directory / "receipt.public.json", value)


def guard(directory, commit, name):
    binding = job(directory, commit, name)
    value = checked(document(directory / "receipt.json"), binding)
    if not value["worker_cleanup_verified"] or value != collect(directory, commit):
        raise ValueError("fixture cleanup unproved")


def archived(directory, commit, name):
    if directory.parent.name != "done": raise ValueError("fixture archive is not done")
    binding = job(directory, commit, name)
    value = checked(document(directory / "receipt.public.json"), binding)
    if value != document(directory / "receipt.json") or value != collect(directory, commit, current=False):
        raise ValueError("fixture archive differs from retained proofs")
    return value
