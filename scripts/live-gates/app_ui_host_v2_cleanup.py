"""V2 cleanup requires independent supervisor-owned host and driver exits."""
import json
from pathlib import Path
import re
import sys

from app_ui_diagnostic import digest, unique_object
from app_ui_host_cleanup import finite_number
from app_ui_host_v2_bundle import ROLES, verify_bundles
from app_ui_host_v2_manifest import TIER, parse_manifest, safe_regular

ROLE_KEYS = {"pid", "launch_date", "bundle_identifier", "bundle_path", "executable_path",
             "executable_sha256", "identity_verified", "exit_observed", "cleanup_verified",
             "termination", "launch_requested", "no_launch_verified", "failure"}
TOP_KEYS = {"schema_version", "kind", "started_uptime", "deadline_uptime", "host", "driver",
            "cleanup_verified", "success", "failure", "cancelled", "timed_out"}


def read_json(path, required, optional=(), maximum=65_536):
    value = json.loads(safe_regular(path, maximum).read_text(), object_pairs_hook=unique_object,
                       parse_constant=lambda _: (_ for _ in ()).throw(ValueError("nonfinite JSON")))
    if not isinstance(value, dict) or not set(required) <= set(value) or set(value) - set(required) - set(optional):
        raise ValueError("v2 lifecycle schema differs")
    return value


def load_job(output, commit, manifest, expected):
    fields = {}
    for line in safe_regular(output / "job.env", 16_384).read_text().splitlines():
        key, separator, value = line.partition("=")
        if not separator or key in fields:
            raise ValueError("invalid job record")
        fields[key] = value
    if (fields.get("tier") != TIER or fields.get("commit") != commit
            or fields.get("input_manifest_sha256") != digest(manifest)
            or fields.get("sealed_binary_sha256") != expected
            or not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]{0,127}", fields.get("job_id", ""))):
        raise ValueError("v2 job seals differ")
    return fields["job_id"]


def verify_role(role, value, private, fields):
    if not isinstance(value, dict) or set(value) != ROLE_KEYS:
        raise ValueError("v2 role schema differs")
    name, executable, identifier, key = ROLES[role]
    bundle = private / name
    if (value["bundle_identifier"] != identifier or value["bundle_path"] != str(bundle)
            or value["executable_path"] != str(bundle / "Contents/MacOS" / executable)
            or value["executable_sha256"] != fields[key][1]
            or value["termination"] not in ("none", "term", "kill")
            or not (value["failure"] is None or isinstance(value["failure"], str))
            or any(type(value[k]) is not bool for k in ("identity_verified", "exit_observed",
                   "cleanup_verified", "launch_requested", "no_launch_verified"))):
        raise ValueError("v2 role identity or flags differ")
    if value["launch_requested"]:
        if (type(value["pid"]) is not int or value["pid"] < 2
                or not finite_number(value["launch_date"], 0)
                or value["no_launch_verified"] is not False
                or any(value[k] is not True for k in ("identity_verified", "exit_observed", "cleanup_verified"))):
            raise ValueError("v2 requested role lacks independent owned exit")
    elif (value["pid"] is not None or value["launch_date"] is not None
          or value["identity_verified"] is not False or value["exit_observed"] is not False
          or value["termination"] != "none" or value["no_launch_verified"] is not True
          or value["cleanup_verified"] is not True):
        raise ValueError("v2 role has no proven no-launch outcome")


def verify_launcher_exit(path, private, fields):
    if private != private.resolve(strict=True):
        raise ValueError("v2 private directory is not canonical")
    launch = read_json(path, TOP_KEYS)
    if (type(launch["schema_version"]) is not int or launch["schema_version"] != 2
            or launch["kind"] != "native-app-ui-launcher-v2"
            or not finite_number(launch["started_uptime"], 0)
            or not finite_number(launch["deadline_uptime"], 0)
            or abs(launch["deadline_uptime"] - launch["started_uptime"] - 90) > 0.000001
            or any(type(launch[k]) is not bool for k in ("cleanup_verified", "success", "cancelled", "timed_out"))
            or not (launch["failure"] is None or isinstance(launch["failure"], str))):
        raise ValueError("v2 supervisor schema or deadline differs")
    for role in ROLES:
        verify_role(role, launch[role], private, fields)
    if launch["host"]["pid"] is not None and launch["host"]["pid"] == launch["driver"]["pid"]:
        raise ValueError("v2 application identities collide")
    if launch["cleanup_verified"] is not True:
        raise ValueError("v2 supervisor cleanup is unresolved")
    if launch["success"] and (launch["failure"] is not None or launch["cancelled"] or launch["timed_out"]
            or any(not launch[r]["launch_requested"] or launch[r]["failure"] is not None
                   or launch[r]["termination"] != "none" for r in ROLES)):
        raise ValueError("v2 supervisor success contradicts lifecycle evidence")
    verify_bundles(private, fields, signatures=False)
    return launch


def verify_job_cleanup(output, commit):
    output = Path(output)
    manifest = output / "input-manifest.tsv"
    fields = parse_manifest(manifest, commit)
    expected = fields["binary"][1]
    job_id = load_job(output, commit, manifest, expected)
    receipt = read_json(output / "receipt.json", ("tier", "commit", "job_id", "binary_hash",
        "input_manifest_sha256", "helper_sha256", "run_count", "claim_eligible", "criterion_pass",
        "capability_promotion"), ("boots_attempted", "sample_count", "passes", "failures", "pass",
        "worker_cleanup_verified", "outcome", "failure_code", "result_sha256", "driver_result_sha256"))
    if (receipt["tier"] != TIER or receipt["commit"] != commit or receipt["job_id"] != job_id
            or receipt["binary_hash"] != expected or receipt["input_manifest_sha256"] != digest(manifest)
            or receipt["helper_sha256"] != fields["launcher"][1]
            or type(receipt["run_count"]) is not int or receipt["run_count"] not in (0, 1)
            or any(receipt[k] is not False for k in ("claim_eligible", "criterion_pass", "capability_promotion"))):
        raise ValueError("v2 cleanup receipt is not bound to this job")
    private = output / "app-ui-private"
    if receipt["run_count"] == 0 and not private.exists() and not private.is_symlink():
        return
    if receipt["run_count"] != 1 or not private.is_dir() or private.is_symlink():
        raise ValueError("v2 prelaunch cleanup cannot be established")
    verify_launcher_exit(private / "launcher-observations-v2.json", private, fields)


if __name__ == "__main__":
    try:
        if len(sys.argv) != 3:
            raise ValueError("expected JOB_DIRECTORY COMMIT")
        verify_job_cleanup(*sys.argv[1:])
    except (OSError, ValueError) as error:
        print(f"native dual-app cleanup unresolved: {error}", file=sys.stderr)
        sys.exit(126)
