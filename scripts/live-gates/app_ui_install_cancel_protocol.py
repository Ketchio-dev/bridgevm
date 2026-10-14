"""Independent process/session binding for the install cancellation protocol."""
import math
import re

from app_ui_diagnostic import digest
from app_ui_host_v2_bundle import ROLES
from app_ui_host_v2_diagnostic import protocol_identity
from app_ui_install_cancel_json import protocol_record
from app_ui_install_cancel_transcript import verify_transcript


def verify_driver(private, launch):
    path = private / "install-cancel-session.json"
    session = protocol_record(path, "native-app-ui-install-cancel-session",
        {"nonce", "startedUptime", "deadlineUptime", "host", "driver"})
    if (not isinstance(session["nonce"], str) or not re.fullmatch(r"[0-9a-f]{64}", session["nonce"])
            or type(session["startedUptime"]) not in (int, float)
            or type(session["deadlineUptime"]) not in (int, float)
            or session["startedUptime"] != launch["started_uptime"]
            or session["deadlineUptime"] != launch["deadline_uptime"]
            or session["deadlineUptime"] - session["startedUptime"] != 90
            or any(session[role] != protocol_identity(launch[role]) for role in ROLES)):
        raise ValueError("install driver session differs from independent owner identities")
    if any(type(session[role]["pid"]) is not int or not 2 <= session[role]["pid"] <= 2 ** 31 - 1
           or type(session[role]["launchDate"]) not in (int, float)
           or not math.isfinite(session[role]["launchDate"]) or session[role]["launchDate"] <= 0 for role in ROLES):
        raise ValueError("install process identity differs from Swift numeric types")
    session_hash = digest(path)
    ready = protocol_record(private / "install-cancel-ready.json", "native-app-ui-install-cancel-ready",
        {"sessionSHA256", "nonce", "driver", "trusted"},
        {"failureCode", "codeRequirement", "codeRequirementUnavailableReason"})
    completion_path = private / "install-cancel-completion.json"
    completion = protocol_record(completion_path, "native-app-ui-install-cancel-completion",
        {"sessionSHA256", "nonce", "hostPID", "driverPID", "success", "requestsProcessed", "mutationsPerformed"},
        {"failureCode"})
    for record in (ready, completion):
        if (record["sessionSHA256"] != session_hash or record["nonce"] != session["nonce"]
                or record.get("failureCode") is not None):
            raise ValueError("install driver result failed or belongs to another session")
    requirements = [(ready.get("codeRequirement"), 4096), (ready.get("codeRequirementUnavailableReason"), 128)]
    if (sum(value is not None for value, _ in requirements) != 1
            or any(value is not None and (not isinstance(value, str) or not 0 < len(value.encode()) <= limit or "\0" in value)
                   for value, limit in requirements)
            or ready["trusted"] is not True or ready["driver"] != session["driver"]):
        raise ValueError("install driver trust or code identity differs")
    if (completion["success"] is not True
            or any(type(completion[key]) is not int or completion[key] != launch[role]["pid"]
                   for key, role in (("hostPID", "host"), ("driverPID", "driver")))
            or type(completion["requestsProcessed"]) is not int or not 8 <= completion["requestsProcessed"] <= 1024
            or type(completion["mutationsPerformed"]) is not int or completion["mutationsPerformed"] != 3):
        raise ValueError("install driver has no complete fixed-scenario result")
    reads = verify_transcript(private, session, session_hash, completion["requestsProcessed"])
    return digest(completion_path), reads
