"""Fail-closed public receipt boundary for D5, never a release criterion."""
import json
import os
from pathlib import Path
import re

from guest_input_protocol import regular_bytes

SCHEMA = "bridgevm.guest-input-queue.v1"
BOOLS = ("complete", "source_integrity", "cleanup_complete", "clean_shutdown", "guest_application_observed")


def job(directory, commit):
    entries = {}
    for line in regular_bytes(directory / "job.env", 16384).decode("ascii").splitlines():
        key, separator, value = line.partition("=")
        if not separator or key in entries:
            raise ValueError("invalid job envelope")
        entries[key] = value
    if (entries.get("tier") != "d5-guest-input" or entries.get("commit") != commit
            or not re.fullmatch(r"[0-9a-f]{40}", commit)
            or not re.fullmatch(r"[a-zA-Z0-9][a-zA-Z0-9._-]{0,95}", entries.get("job_id", ""))):
        raise ValueError("job identity mismatch")
    for key in ("input_manifest_sha256", "sealed_binary_sha256"):
        if not re.fullmatch(r"[0-9a-f]{64}", entries.get(key, "")):
            raise ValueError("missing job seal")
    return entries


def empty(identity, reason):
    result = {"schema": SCHEMA, "tier": "d5-guest-input", "job_id": identity["job_id"],
              "commit": identity["commit"], "input_manifest_sha256": identity["input_manifest_sha256"],
              "sealed_binary_sha256": identity["sealed_binary_sha256"], "reason": reason,
              "claim_eligible": False, "criterion_pass": False, "passed": False,
              "production_ui_proven": False}
    result.update({key: False for key in BOOLS})
    return result


def checked(value, identity):
    base = empty(identity, "incomplete")
    if not isinstance(value, dict) or set(value) != set(base):
        raise ValueError("unexpected receipt fields")
    for key, expected in base.items():
        if key in BOOLS:
            if type(value[key]) is not bool:
                raise ValueError("invalid receipt boolean")
        elif key == "reason":
            if value[key] not in {"incomplete", "collected", "canceled", "invalid-receipt"}:
                raise ValueError("invalid reason")
        elif type(value[key]) is not type(expected) or value[key] != expected:
            raise ValueError("receipt identity or claim mismatch")
    if value["guest_application_observed"] and not all(value[key] for key in BOOLS):
        raise ValueError("unsupported guest observation")
    if value["reason"] != "collected" and any(value[key] for key in BOOLS):
        raise ValueError("incomplete receipt carries success state")
    return value


def write_new(path, value):
    with path.open("x", encoding="utf-8") as stream:
        stream.write(json.dumps(value, sort_keys=True) + "\n")


def finalize(directory, commit):
    identity = job(directory, commit)
    receipt = directory / "receipt.json"
    reason = "canceled" if (directory / "cancel.requested").exists() else "incomplete"
    if receipt.exists() or receipt.is_symlink():
        try:
            checked(json.loads(regular_bytes(receipt, 16384)), identity)
            if reason != "canceled":
                return
        except (OSError, ValueError):
            reason = "invalid-receipt"
        archived = directory / "receipt.before-finalize.json"
        if archived.exists() or archived.is_symlink():
            raise ValueError("refusing to overwrite prior receipt evidence")
        os.rename(receipt, archived)
    write_new(receipt, empty(identity, reason))


def publish(directory, commit):
    identity = job(directory, commit)
    value = checked(json.loads(regular_bytes(directory / "receipt.json", 16384)), identity)
    if (directory / "cancel.requested").exists() and value["reason"] != "canceled":
        raise ValueError("cancellation not finalized")
    write_new(directory / "receipt.public.json", value)
