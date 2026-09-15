"""Verify owned host exit independently of whether its UI scenario succeeded."""
import json
import math
from pathlib import Path
import re
import sys

from app_ui_diagnostic import digest, regular, unique_object
from app_ui_host_manifest import TIER, parse_manifest, verify_executable

BUNDLE_ID = "dev.bridgevm.app-ui-host"


def load_job(output, commit, manifest, binary_digest):
    fields = {}
    for line in regular(output / "job.env", 16_384).read_text().splitlines():
        key, separator, value = line.partition("=")
        if not separator or key in fields:
            raise ValueError("invalid job record")
        fields[key] = value
    if (fields.get("tier") != TIER or fields.get("commit") != commit
            or fields.get("input_manifest_sha256") != digest(manifest)
            or fields.get("sealed_binary_sha256") != binary_digest
            or not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]{0,127}", fields.get("job_id", ""))):
        raise ValueError("job seals differ")
    return fields["job_id"]


def read_object(path, keys):
    value = json.loads(regular(path, 65_536).read_text(), object_pairs_hook=unique_object)
    if not isinstance(value, dict) or set(value) != set(keys):
        raise ValueError("host lifecycle schema differs")
    if type(value["schema_version"]) is not int or value["schema_version"] != 1:
        raise ValueError("host lifecycle version differs")
    return value


def finite_number(value, minimum):
    return type(value) in (int, float) and math.isfinite(value) and value >= minimum


def verify_launcher_exit(path, bundle, expected):
    launch = read_object(path, (
        "schema_version", "kind", "host_pid", "host_launch_date", "host_bundle_path",
        "host_executable_path", "host_executable_sha256", "identity_verified", "exit_observed",
        "cleanup_verified", "cancelled", "timed_out", "termination", "success", "failure"))
    bundle_path = str(bundle.resolve(strict=True))
    executable = bundle / "Contents/MacOS/BridgeVMControl"
    if (launch["kind"] != "native-app-ui-launcher"
            or type(launch["host_pid"]) is not int or launch["host_pid"] < 2
            or not finite_number(launch["host_launch_date"], 0)
            or launch["host_bundle_path"] != bundle_path
            or launch["host_executable_path"] != str(executable.resolve(strict=True))
            or launch["host_executable_sha256"] != expected
            or any(launch[key] is not True for key in ("identity_verified", "exit_observed", "cleanup_verified"))
            or any(type(launch[key]) is not bool for key in ("success", "cancelled", "timed_out"))
            or launch["termination"] not in ("none", "term", "kill")
            or not (launch["failure"] is None or isinstance(launch["failure"], str))):
        raise ValueError("launcher has no verified owned exit")
    if launch["success"] != (launch["failure"] is None and not launch["cancelled"] and not launch["timed_out"]):
        raise ValueError("launcher lifecycle flags disagree")
    verify_executable(executable, expected)
    return launch


def verify_job_cleanup(output, commit):
    output = Path(output)
    manifest = output / "input-manifest.tsv"
    fields = parse_manifest(manifest, commit)
    expected = fields["binary"][1]
    job_id = load_job(output, commit, manifest, expected)
    receipt = json.loads(regular(output / "receipt.json", 65_536).read_text(), object_pairs_hook=unique_object)
    if (not isinstance(receipt, dict) or receipt.get("tier") != TIER or receipt.get("commit") != commit
            or receipt.get("job_id") != job_id or receipt.get("binary_hash") != expected
            or receipt.get("input_manifest_sha256") != digest(manifest)
            or receipt.get("helper_sha256") != fields["launcher"][1]
            or type(receipt.get("run_count")) is not int or receipt["run_count"] not in (0, 1)
            or any(receipt.get(key) is not False for key in ("claim_eligible", "criterion_pass", "capability_promotion"))):
        raise ValueError("cleanup receipt is absent or not bound to this job")
    private = output / "app-ui-private"
    if receipt["run_count"] == 0 and not private.exists() and not private.is_symlink():
        return
    if receipt["run_count"] != 1 or not private.is_dir() or private.is_symlink():
        raise ValueError("prelaunch cleanup cannot be established")
    verify_launcher_exit(private / "launcher-observations.json", private / "BridgeVMAppUIHost.app", expected)


if __name__ == "__main__":
    try:
        if len(sys.argv) != 3:
            raise ValueError("expected JOB_DIRECTORY COMMIT")
        verify_job_cleanup(*sys.argv[1:])
    except (OSError, ValueError) as error:
        print(f"native app cleanup unresolved: {error}", file=sys.stderr)
        sys.exit(126)
