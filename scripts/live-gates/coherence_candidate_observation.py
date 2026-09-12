"""Candidate enumeration is separate from current resident-agent WINLIST."""
import re
from coherence_inventory_observation import fixture as original_fixture, compare as original_compare, inventory


def fixture(value, nonce):
    original_fixture(value, nonce)
    candidate = value.get("candidate")
    if (not isinstance(candidate, dict) or set(candidate) != {"completed", "handles", "failure"}
            or type(candidate["completed"]) is not bool or not isinstance(candidate["handles"], list)
            or candidate["failure"] != ("none" if candidate["completed"] else "EnumerationFailed")):
        raise ValueError("invalid candidate result")
    handles = candidate["handles"]
    wanted = {window["id"] for window in value["windows"]}
    if (any(not isinstance(handle, str) or handle not in wanted for handle in handles)
            or len(set(handles)) != len(handles) or (not candidate["completed"] and handles)):
        raise ValueError("invalid candidate observed handles")
    version = value.get("powershell_version")
    if (not isinstance(version, str) or not re.fullmatch(r"[0-9]+(?:\.[0-9]+){1,3}", version)
            or value.get("process_architecture") not in ("ARM64", "AMD64", "x86")):
        raise ValueError("invalid candidate runtime identity")
    return value


def compare(expected, response):
    result = original_compare(expected, response)
    candidate = expected["candidate"]
    count = len(candidate["handles"])
    result["candidate"] = {"collection_complete": candidate["completed"], "observed_windows": count,
        "expected_windows": len(expected["windows"]), "all_fixture_windows_observed": candidate["completed"] and count == len(expected["windows"]),
        "failure": candidate["failure"], "claim_eligible": False, "powershell_version": expected["powershell_version"],
        "process_architecture": expected["process_architecture"],
        "target_runtime": expected["powershell_version"].startswith("5.1.") and expected["process_architecture"] == "ARM64"}
    return result
