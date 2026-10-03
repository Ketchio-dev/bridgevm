"""Fixed non-promoting D4 receipt contract, including required output bounds."""
import re

TIER = "d4-winpe-companions"
FLAGS = ("pass", "claim_eligible", "criterion_pass", "capability_promotion", "winpe_boot_proven")
SAFETY = ("cleanup_complete", "mount_cleanup_complete")

def initial(job):
    return {"tier": TIER, **{key: job[key] for key in ("job_id", "commit", "input_manifest_sha256")},
            **{key: False for key in FLAGS + SAFETY}, "outcome": "diagnostic-incomplete",
            "sample_count": 0, "required_run_count": 1, "passes": 0,
            "source_integrity_verified": False, "post_files_match": False,
            "files_changed": False, "pre_post_available": False,
            "output_bounds_verified": False, "output_bounds_refused": False}


def validate(data, job):
    if job.get("tier") != TIER or data.get("tier") != TIER:
        raise ValueError("invalid diagnostic tier")
    for key, pattern in (("job_id", r"[A-Za-z0-9][A-Za-z0-9._-]{0,127}"),
                         ("commit", r"[0-9a-f]{40}"), ("input_manifest_sha256", r"[0-9a-f]{64}")):
        if data.get(key) != job.get(key) or not re.fullmatch(pattern, data.get(key, "")):
            raise ValueError("invalid sealed identity")
    if any(data.get(key) is not False for key in FLAGS):
        raise ValueError("diagnostic cannot promote a claim")
    for key, allowed in (("passes", (0,)), ("sample_count", (0, 1)), ("required_run_count", (1,))):
        if type(data.get(key)) is not int or data[key] not in allowed:
            raise ValueError("invalid diagnostic count")
    for key in ("source_integrity_verified", "post_files_match", "files_changed", "pre_post_available", "output_bounds_verified", "output_bounds_refused") + SAFETY:
        if type(data.get(key)) is not bool:
            raise ValueError("invalid observation flag")
    if data.get("outcome") not in ("diagnostic-incomplete", "diagnostic-complete", "canceled"):
        raise ValueError("invalid diagnostic outcome")
    if data["output_bounds_refused"] and data["output_bounds_verified"]:
        raise ValueError("refused output cannot be verified")
    if data["outcome"] == "diagnostic-complete":
        if not data["output_bounds_verified"] or data["output_bounds_refused"]:
            raise ValueError("bounded output is unproven")
        if data["sample_count"] != 1 or not data["source_integrity_verified"] or not all(data[key] for key in SAFETY):
            raise ValueError("incomplete collection")
        if type(data.get("execution_exit_code")) is not int or not -255 <= data["execution_exit_code"] <= 255:
            raise ValueError("invalid process result")
