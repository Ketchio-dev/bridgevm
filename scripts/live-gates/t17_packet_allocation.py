"""Retain exact request bytes so allocation bindings survive work/job movement."""
import hashlib
import os
import re
import product_e2e_work as WORK


def retain(packet, lane, device, private, stack, created, ordinal, expected_hash):
    fd, info = packet["source_file"](lane, "request.json", device, stack)
    if not 0 < info.st_size <= packet["JSON_CAP"]:
        raise ValueError("request retention size differs")
    raw = packet["read_exact"](fd, 0, info.st_size)
    packet["unchanged"](lane, "request.json", fd, info)
    if hashlib.sha256(raw).hexdigest() != expected_hash:
        raise ValueError("request changed before retention")
    out = packet["destination"](private, f"t17-diagnostic-lane-{ordinal}-request.json", stack, created)
    packet["write_all"](out, raw)
    os.fsync(out)


def verify(packet, private, device, stack, index, ordinal):
    request, sha = packet["read_json"](private, f"t17-diagnostic-lane-{ordinal}-request.json", device, stack)
    if sha != index["request_sha256"]:
        raise ValueError("retained request differs from host stamp")
    for key in (*WORK.FIELDS, "lane_root", "job_id", "lane", "commit", "nonce"):
        if type(request.get(key)) is not type(index[key]) or request.get(key) != index[key]:
            raise ValueError("allocation differs from retained sealed request")
    for key in ("work_parent_identity", "work_identity"):
        if not isinstance(index[key], str) or not re.fullmatch(r"[0-9]+:[0-9]+", index[key]):
            raise ValueError("invalid retained allocation identity")
    WORK.layout(index["lane_root"], index["work_parent"], index["job_id"], "e2e", ordinal)
