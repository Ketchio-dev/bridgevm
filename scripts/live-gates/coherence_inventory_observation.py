"""Strict nonce-scoped WINLIST comparison with owned fixture windows."""
import base64
import hashlib
import json


def fixture(value, nonce):
    if (not isinstance(value, dict) or value.get("schema") != "bridgevm.coherence-fixture.v1"
            or value.get("nonce") != nonce or type(value.get("pid")) is not int
            or not 0 < value["pid"] <= 0xffffffff or not isinstance(value.get("windows"), list)
            or len(value["windows"]) != 2):
        raise ValueError("invalid multiwindow fixture")
    handles = []
    for index, window in enumerate(value["windows"]):
        if not isinstance(window, dict) or set(window) != {"id", "title"}:
            raise ValueError("invalid fixture window")
        handle = window["id"]
        if (not isinstance(handle, str) or not handle.isascii() or not handle.isdecimal()
                or not 0 < int(handle) < 2**64 or str(int(handle)) != handle
                or window["title"] != "BridgeVM Coherence " + nonce + " " + str(index)):
            raise ValueError("invalid fixture handle/title")
        handles.append(handle)
    if len(set(handles)) != 2:
        raise ValueError("fixture handles must be distinct")
    return value


def inventory(lines, command):
    if any(line.startswith(("BVAGENT READY", "BVAGENT re-READY", "BVAGENT SERVICE start", "PSCI_SYSTEM_RESET", "PSCI SYSTEM_RESET:")) for line in lines):
        raise ValueError("guest restarted during inventory")
    prefix = "BVAGENT " + command + " "
    rows, handles, size = [], set(), 0
    for line in lines:
        if not line.startswith(prefix):
            continue
        payload = line[len(prefix):].removesuffix("\r")
        if payload == "WINEND":
            return {"rows": rows}
        size += len(line.encode())
        if size > 4 * 1024 * 1024 or len(rows) >= 4096:
            raise ValueError("inventory exceeds diagnostic bound")
        parts = payload.split(" ", 7)
        if len(parts) != 8 or parts[0] != "WIN":
            raise ValueError("malformed inventory row")
        handle, pid, x, y, width, height = map(int, parts[1:7])
        if not 0 < handle < 2**64 or not 0 < pid <= 0xffffffff or width <= 0 or height <= 0:
            raise ValueError("invalid window fields")
        if handle in handles:
            raise ValueError("duplicate inventory handle")
        handles.add(handle)
        title = base64.b64decode(parts[7], validate=True).decode("utf-8")
        rows.append({"id": str(handle), "pid": pid, "title": title})
    return None


def compare(expected, response):
    wanted = {window["id"]: window["title"] for window in expected["windows"]}
    seen = {row["id"] for row in response["rows"]
            if row["pid"] == expected["pid"] and wanted.get(row["id"]) == row["title"]}
    return {"schema": "bridgevm.coherence-observation.v1", "nonce": expected["nonce"],
            "collection_complete": True, "expected_windows": len(wanted), "observed_windows": len(seen),
            "all_fixture_windows_observed": len(seen) == len(wanted), "claim_eligible": False,
            "fixture_sha256": hashlib.sha256(json.dumps(expected, sort_keys=True).encode()).hexdigest()}
