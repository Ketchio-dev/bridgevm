#!/usr/bin/env python3
"""Sealed dual-app scenario: UI success and independent cleanup are separate."""
import os
from pathlib import Path
import re
import signal
import subprocess
import sys
import json

from app_ui_diagnostic import digest, verify_observations
from app_ui_host_cleanup import finite_number
from app_ui_host_process import request_cancel
from app_ui_host_v2_bundle import ROLES, reconstruct_bundles, verify_bundles
from app_ui_host_v2_cleanup import load_job, read_json, verify_launcher_exit
from app_ui_host_v2_manifest import FILES, TIER, parse_manifest, verify_input
from app_ui_host_v2_process import run_launcher


def protocol_record(path, kind, required, optional=()):
    value = read_json(path, {"schemaVersion", "kind", *required}, optional, maximum=8192)
    if type(value["schemaVersion"]) is not int or value["schemaVersion"] != 1 or value["kind"] != kind:
        raise ValueError("v2 driver protocol version or kind differs")
    return value


def protocol_identity(role):
    return {"pid": role["pid"], "launchDate": role["launch_date"],
            "bundleIdentifier": role["bundle_identifier"], "bundlePath": role["bundle_path"],
            "executablePath": role["executable_path"], "executableSHA256": role["executable_sha256"]}


def verify_driver(private, launch):
    session_path = private / "driver-session.json"
    session = protocol_record(session_path, "native-app-ui-driver-session",
                              {"nonce", "startedUptime", "deadlineUptime", "host", "driver"})
    if (not isinstance(session["nonce"], str) or not re.fullmatch(r"[0-9a-f]{64}", session["nonce"])
            or type(session["startedUptime"]) not in (int, float)
            or type(session["deadlineUptime"]) not in (int, float)
            or session["startedUptime"] != launch["started_uptime"]
            or session["deadlineUptime"] != launch["deadline_uptime"]
            or any(session[r] != protocol_identity(launch[r]) for r in ROLES)):
        raise ValueError("v2 driver session differs from independent owner identities")
    session_hash = digest(session_path)
    ready = protocol_record(private / "driver-ready.json", "native-app-ui-driver-ready",
        {"sessionSHA256", "nonce", "driver", "trusted"},
        {"failureCode", "codeRequirement", "codeRequirementUnavailableReason"})
    completion_path = private / "driver-completion.json"
    completion = protocol_record(completion_path, "native-app-ui-driver-completion",
        {"sessionSHA256", "nonce", "hostPID", "driverPID", "success", "requestsProcessed", "mutationsPerformed"},
        {"failureCode"})
    for record in (ready, completion):
        if (record["sessionSHA256"] != session_hash or record["nonce"] != session["nonce"]
                or record.get("failureCode") is not None):
            raise ValueError("v2 driver result is failed or belongs to another session")
    requirements = [(ready.get("codeRequirement"), 4096), (ready.get("codeRequirementUnavailableReason"), 128)]
    if (sum(value is not None for value, _ in requirements) != 1
            or any(value is not None and (not isinstance(value, str) or not 0 < len(value.encode()) <= limit or "\0" in value)
                   for value, limit in requirements)
            or ready["trusted"] is not True or ready["driver"] != session["driver"]):
        raise ValueError("v2 driver trust or code identity record differs")
    if (completion["success"] is not True
            or type(completion["hostPID"]) is not int or completion["hostPID"] != launch["host"]["pid"]
            or type(completion["driverPID"]) is not int or completion["driverPID"] != launch["driver"]["pid"]
            or type(completion["requestsProcessed"]) is not int or not 12 <= completion["requestsProcessed"] <= 1024
            or type(completion["mutationsPerformed"]) is not int or completion["mutationsPerformed"] != 6):
        raise ValueError("v2 driver has no complete fixed-scenario result")
    return digest(completion_path)


def verify_result(private, fields, launcher_exit):
    launch = verify_launcher_exit(private / "launcher-observations-v2.json", private, fields)
    if launcher_exit != 0 or launch["success"] is not True:
        raise ValueError("v2 supervisor did not complete successfully")
    driver_hash = verify_driver(private, launch)
    observations = private / "host-observations"
    report_hash = verify_observations(observations)
    identity = read_json(observations / "host-identity.json", ("schema_version", "kind", "pid",
        "bundle_identifier", "bundle_path", "executable_path", "executable_sha256", "started_uptime"))
    owner = launch["host"]
    if (type(identity["schema_version"]) is not int or identity["schema_version"] != 1
            or identity["kind"] != "native-app-ui-host-identity" or type(identity["pid"]) is not int
            or any(identity[k] != owner[k] for k in ("pid", "bundle_identifier", "bundle_path", "executable_path", "executable_sha256"))
            or not finite_number(identity["started_uptime"], 0)):
        raise ValueError("v2 host identity differs")
    completion = read_json(observations / "host-completion.json", ("schema_version", "kind", "pid",
        "success", "cleanup_verified", "report_sha256", "failure"))
    if (type(completion["schema_version"]) is not int or completion["schema_version"] != 1
            or completion["kind"] != "native-app-ui-host-completion" or type(completion["pid"]) is not int
            or completion["pid"] != owner["pid"] or completion["success"] is not True
            or completion["cleanup_verified"] is not True or completion["report_sha256"] != report_hash
            or completion["failure"] is not None):
        raise ValueError("v2 host completion differs")
    verify_bundles(private, fields)
    return report_hash, driver_hash


def run(output, repo, commit, manifest, binary):
    output, repo, manifest, binary = map(Path, (output, repo, manifest, binary))
    receipt = {"tier": TIER, "commit": commit, "claim_eligible": False, "criterion_pass": False,
        "capability_promotion": False, "boots_attempted": 0, "run_count": 0, "sample_count": 0,
        "passes": 0, "failures": 0, "pass": False, "worker_cleanup_verified": False,
        "outcome": "diagnostic-incomplete", "failure_code": "preflight-refused"}
    interrupted = False

    def stop(_signum, _frame):
        nonlocal interrupted
        interrupted = True

    previous = {kind: signal.signal(kind, stop) for kind in (signal.SIGTERM, signal.SIGINT)}
    try:
        if not output.is_dir() or output != output.resolve(strict=True):
            raise ValueError("job directory is not canonical")
        fields = parse_manifest(manifest, commit)
        expected = fields["binary"][1]
        receipt.update(input_manifest_sha256=digest(manifest), binary_hash=expected,
                       helper_sha256=fields["launcher"][1])
        receipt["job_id"] = load_job(output, commit, manifest, expected)
        if binary != output / FILES["binary"]:
            raise ValueError("v2 runtime binary must be the fixed sealed job copy")
        for key in FILES:
            verify_input(key, output / FILES[key], fields[key][1])
        actual = subprocess.check_output(["git", "-C", str(repo), "rev-parse", "HEAD"], text=True, timeout=5).strip()
        if actual != commit or sys.platform != "darwin":
            raise ValueError("requires sealed revision on macOS")
        canceled = lambda: interrupted or os.path.lexists(output / "cancel.requested")
        if canceled():
            raise TimeoutError("canceled before dual-app launch")
        private = output / "app-ui-private"
        private.mkdir(mode=0o700)
        reconstruct_bundles(private, output, repo, commit, fields)
        (private / "host-observations").mkdir(mode=0o700)
        if canceled():
            request_cancel(private)
            raise TimeoutError("canceled before dual-app launch")
        receipt.update(run_count=1, failure_code="native-ui-driver-failed")
        code = run_launcher(output / FILES["launcher"], private, expected, fields["launcher"][1], canceled)
        receipt["failure_code"] = "observation-refused"
        result_hash, driver_hash = verify_result(private, fields, code)
        if canceled():
            raise TimeoutError("canceled before accepting dual-app observations")
        receipt.update(result_sha256=result_hash, driver_result_sha256=driver_hash, outcome="diagnostic-complete",
                       failure_code="none", sample_count=1, passes=1, worker_cleanup_verified=True, **{"pass": True})
    except TimeoutError as error:
        receipt.update(failure_code="canceled-or-deadline")
        print(str(error), file=sys.stderr)
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        print(f"v2 app UI diagnostic refused: {error}", file=sys.stderr)
    finally:
        if receipt["run_count"]:
            try:
                verify_launcher_exit(private / "launcher-observations-v2.json", private, fields)
                receipt["worker_cleanup_verified"] = True
            except (OSError, ValueError):
                receipt["worker_cleanup_verified"] = False
        for kind, handler in previous.items():
            signal.signal(kind, handler)
        if receipt["run_count"] and not receipt["pass"]:
            receipt["failures"] = 1
        with (output / "receipt.json").open("x", encoding="utf-8") as result:
            os.fchmod(result.fileno(), 0o600)
            json.dump(receipt, result, indent=2, sort_keys=True)
            result.write("\n")
    return 0 if receipt["pass"] else 1


if __name__ == "__main__":
    if len(sys.argv) != 7 or sys.argv[1] != "run":
        sys.exit("usage: app_ui_host_v2_diagnostic.py run OUT REPO COMMIT MANIFEST BINARY")
    sys.exit(run(*sys.argv[2:]))
