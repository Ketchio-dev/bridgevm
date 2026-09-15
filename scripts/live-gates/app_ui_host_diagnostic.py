#!/usr/bin/env python3
"""Run the sealed normal-app host; never boot or operate on a guest."""
import json
import os
from pathlib import Path
import plistlib
import shutil
import signal
import subprocess
import sys

from app_ui_diagnostic import digest, verify_observations
from app_ui_host_manifest import LAUNCHER, TIER, parse_manifest, verify_executable
from app_ui_host_cleanup import BUNDLE_ID, finite_number, load_job, read_object, verify_launcher_exit
from app_ui_host_process import request_cancel, run_launcher

RESOURCES = (
    "windows-boot-seed-vars.fd.gz",
    "secureboot-microsoft-windows-transition-aarch64-v1.6.5.json",
)


def reconstruct_bundle(private, repo, commit, binary, expected):
    bundle = private / "BridgeVMAppUIHost.app"
    executable = bundle / "Contents/MacOS/BridgeVMControl"
    executable.parent.mkdir(parents=True)
    shutil.copyfile(binary, executable)
    executable.chmod(0o500)
    verify_executable(executable, expected)
    with (bundle / "Contents/Info.plist").open("xb") as info:
        plistlib.dump({"CFBundleExecutable": "BridgeVMControl", "CFBundlePackageType": "APPL",
                      "CFBundleIdentifier": BUNDLE_ID, "CFBundleName": "BridgeVM UI Diagnostic",
                      "CFBundleVersion": "1", "CFBundleShortVersionString": "1.0",
                      "LSMinimumSystemVersion": "14.0", "NSHighResolutionCapable": True}, info)
    resources = bundle / "Contents/Resources/BridgeVMApp_BridgeVMControl.bundle"
    resources.mkdir(parents=True)
    for name in RESOURCES:
        reference = f"{commit}:apps/macos/Sources/BridgeVMControl/Resources/{name}"
        size = int(subprocess.check_output(["git", "-C", str(repo), "cat-file", "-s", reference], timeout=5))
        if not 0 < size <= 4 * 1024 * 1024:
            raise ValueError("sealed resource exceeds bounds")
        data = subprocess.check_output(["git", "-C", str(repo), "cat-file", "blob", reference], timeout=5)
        if len(data) != size:
            raise ValueError("sealed resource length differs")
        with (resources / name).open("xb") as target:
            target.write(data)
    return bundle


def verify_result(observations, bundle, expected, launcher_report, launcher_exit):
    report_hash = verify_observations(observations)
    identity = read_object(observations / "host-identity.json", (
        "schema_version", "kind", "pid", "bundle_identifier", "bundle_path",
        "executable_path", "executable_sha256", "started_uptime"))
    bundle_path = str(bundle.resolve(strict=True))
    executable_path = str((bundle / "Contents/MacOS/BridgeVMControl").resolve(strict=True))
    if (identity["kind"] != "native-app-ui-host-identity"
            or type(identity["pid"]) is not int or identity["pid"] < 2
            or identity["bundle_identifier"] != BUNDLE_ID or identity["bundle_path"] != bundle_path
            or identity["executable_path"] != executable_path or identity["executable_sha256"] != expected
            or not finite_number(identity["started_uptime"], 0)):
        raise ValueError("host identity differs")
    completion = read_object(observations / "host-completion.json", (
        "schema_version", "kind", "pid", "success", "cleanup_verified", "report_sha256", "failure"))
    if (completion["kind"] != "native-app-ui-host-completion"
            or type(completion["pid"]) is not int or completion["pid"] != identity["pid"]
            or completion["success"] is not True or completion["cleanup_verified"] is not True
            or completion["report_sha256"] != report_hash or completion["failure"] is not None):
        raise ValueError("host completion differs")
    launch = verify_launcher_exit(launcher_report, bundle, expected)
    if (launcher_exit != 0 or launch["host_pid"] != identity["pid"]
            or launch["success"] is not True or launch["termination"] != "none"
            or launch["failure"] is not None or launch["cancelled"] is not False
            or launch["timed_out"] is not False):
        raise ValueError("launcher ownership or exit differs")
    verify_executable(bundle / "Contents/MacOS/BridgeVMControl", expected)
    return report_hash


def run(output, repo, commit, manifest, binary):
    output, repo, manifest, binary = map(Path, (output, repo, manifest, binary))
    receipt = {"tier": TIER, "commit": commit, "claim_eligible": False, "criterion_pass": False,
               "capability_promotion": False, "boots_attempted": 0, "run_count": 0,
               "sample_count": 0, "passes": 0, "failures": 0, "pass": False,
               "worker_cleanup_verified": False, "outcome": "diagnostic-incomplete", "failure_code": "preflight-refused"}
    interrupted = False

    def stop(_signum, _frame):
        nonlocal interrupted
        interrupted = True

    previous = {kind: signal.signal(kind, stop) for kind in (signal.SIGTERM, signal.SIGINT)}
    try:
        if not output.is_dir() or output.is_symlink():
            raise ValueError("job directory is unavailable")
        fields = parse_manifest(manifest, commit)
        expected = fields["binary"][1]
        launcher = output / LAUNCHER
        receipt.update(input_manifest_sha256=digest(manifest), binary_hash=expected,
                       helper_sha256=fields["launcher"][1])
        receipt["job_id"] = load_job(output, commit, manifest, expected)
        verify_executable(binary, expected)
        verify_executable(launcher, fields["launcher"][1])
        actual = subprocess.check_output(["git", "-C", str(repo), "rev-parse", "HEAD"], text=True, timeout=5).strip()
        if actual != commit or sys.platform != "darwin":
            raise ValueError("requires the sealed revision on macOS")
        canceled = lambda: interrupted or os.path.lexists(output / "cancel.requested")
        if canceled():
            raise TimeoutError("canceled before native app launch")
        private = output / "app-ui-private"
        private.mkdir(mode=0o700)
        private = private.resolve(strict=True)
        bundle = reconstruct_bundle(private, repo, commit, binary, expected)
        observations = private / "host-observations"
        observations.mkdir(mode=0o700)
        launcher_report = private / "launcher-observations.json"
        if canceled():
            request_cancel(private)
            raise TimeoutError("canceled before native app launch")
        receipt.update(run_count=1, failure_code="native-ui-host-failed")
        code = run_launcher(launcher, bundle, observations, expected, launcher_report, canceled)
        receipt["failure_code"] = "observation-refused"
        receipt["result_sha256"] = verify_result(observations, bundle, expected, launcher_report, code)
        if canceled():
            raise TimeoutError("canceled before accepting native app observations")
        receipt.update(outcome="diagnostic-complete", failure_code="none", sample_count=1, passes=1,
                       worker_cleanup_verified=True, **{"pass": True})
    except TimeoutError as error:
        receipt.update(outcome="diagnostic-incomplete", failure_code="canceled-or-deadline")
        print(str(error), file=sys.stderr)
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        print(f"app UI host diagnostic refused: {error}", file=sys.stderr)
    finally:
        if receipt["run_count"]:
            try:
                verify_launcher_exit(launcher_report, bundle, expected)
                receipt["worker_cleanup_verified"] = True
            except (OSError, ValueError):
                receipt["worker_cleanup_verified"] = False
        for kind, handler in previous.items():
            signal.signal(kind, handler)
        if receipt["run_count"] and not receipt["pass"]:
            receipt["failures"] = 1
        with (output / "receipt.json").open("x", encoding="utf-8") as result:
            json.dump(receipt, result, indent=2, sort_keys=True)
            result.write("\n")
    return 0 if receipt["pass"] else 1


if __name__ == "__main__":
    if len(sys.argv) != 7 or sys.argv[1] != "run":
        sys.exit("usage: app_ui_host_diagnostic.py run OUT REPO COMMIT MANIFEST BINARY")
    sys.exit(run(*sys.argv[2:]))
