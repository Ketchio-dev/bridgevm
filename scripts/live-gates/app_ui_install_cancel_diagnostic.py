#!/usr/bin/env python3
"""One fixed accepted-cancel/retry UI diagnostic; never guest or release evidence."""
import json
import os
from pathlib import Path
import signal
import subprocess
import sys

from app_ui_diagnostic import digest
from app_ui_host_process import request_cancel
from app_ui_host_v2_bundle import reconstruct_bundles
from app_ui_host_v2_cleanup import load_job
from app_ui_host_v2_manifest import verify_input
from app_ui_host_v2_process import run_launcher, supervisor_executable
from app_ui_install_cancel_cleanup import verify_exit
from app_ui_install_cancel_manifest import FILES, TIER, parse_manifest
from app_ui_install_cancel_result import verify_result


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
        receipt["job_id"] = load_job(output, commit, manifest, expected, tier=TIER)
        if binary != output / FILES["binary"]:
            raise ValueError("install cancellation runtime binary must be the fixed sealed job copy")
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
        code = run_launcher(supervisor_executable(private), private, expected, fields["launcher"][1], canceled,
            mode="--app-ui-install-cancel-supervisor-v1", log_name="install-cancel-launcher.log")
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
        print(f"install cancellation diagnostic refused: {error}", file=sys.stderr)
    finally:
        if receipt["run_count"]:
            try:
                verify_exit(private, fields)
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
        sys.exit("usage: app_ui_install_cancel_diagnostic.py run OUT REPO COMMIT MANIFEST BINARY")
    sys.exit(run(*sys.argv[2:]))
