"""Offline retained origins: sealed historical requests and current allocations.

Historical request bytes predate allocation metadata under the same schema.
This compatibility is only for retained T22 provenance, never live admission.
"""
import re
import product_e2e_work as WORK
from product_e2e_identity import fixed_fields_match


def validate(request, writer, fixed, tier, stamped_hash, request_hash):
    historical = writer.REQUEST_KEYS - set(WORK.FIELDS)
    if (type(request) is not dict or set(request) not in (historical, writer.REQUEST_KEYS)
            or not fixed_fields_match(request, fixed) or stamped_hash != request_hash
            or any(type(request[k]) is not str or not request[k].startswith("/") for k in writer.REQUEST_PATHS)):
        raise ValueError("retained request is not bound to its stamp")
    if set(request) == historical:
        return
    for key in ("work_parent_identity", "work_identity"):
        if type(request[key]) is not str or not re.fullmatch(r"[0-9]+:[0-9]+", request[key]):
            raise ValueError("invalid retained allocation identity")
    if (type(request["work_parent"]) is not str
            or request["work_parent_identity"].split(":")[0] != request["work_identity"].split(":")[0]):
        raise ValueError("invalid retained allocation parent/device")
    # Only historical spelling and structure: work is normally already gone.
    WORK.layout(request["lane_root"], request["work_parent"], request["job_id"],
                "e2e" if tier == "T17" else "import-e2e", request["lane"])
