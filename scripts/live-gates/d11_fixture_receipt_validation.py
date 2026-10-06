"""Strict D11 stage and public field schema; no capability promotion."""
from d11_fixture_inputs import SHA

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
