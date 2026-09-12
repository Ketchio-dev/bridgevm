#!/usr/bin/env python3
"""Physical-queue execution body; never substitutes for a production UI gate."""
import argparse
import json
import os
from pathlib import Path
import platform
import re
import shutil
import signal
import subprocess
import sys
import time

from guest_input_controller import Controller
from guest_input_live_inputs import load, clone_media, digest, PROFILE
from guest_input_protocol import regular_bytes
from guest_input_live_cleanup import finalize
from guest_input_profile_dispatch import make_controller, sink_filename, observation_succeeded


def interrupted(signum, frame):
    raise InterruptedError("owned live diagnostic interrupted")


def execute(args):
    root = Path(__file__).resolve().parents[2]
    if platform.system() != "Darwin" or platform.machine() != "arm64":
        raise ValueError("physical Apple-silicon Mac required")
    if not re.fullmatch(r"[0-9a-f]{40}", args.commit):
        raise ValueError("exact commit required")
    if not re.fullmatch(r"[a-zA-Z0-9][a-zA-Z0-9._-]{0,95}", args.job_id):
        raise ValueError("invalid job identity")
    head = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip()
    if head != args.commit:
        raise ValueError("job source identity mismatch")
    paths, hashes, manifest_hash = load(Path(args.input_manifest), root)
    profile = json.loads(regular_bytes(Path(args.input_manifest), 16384))["profile"]
    base = (Path.home() / "BridgeVM/work").resolve()
    work = base / ("guest-input-live-" + args.job_id)
    clones = clone_media(paths, hashes, work)
    share, boot = work / "share", work / "boot"
    share.mkdir(mode=0o700)
    control = work / "agent.ctl"
    control.touch(mode=0o600, exist_ok=False)
    sink = root / "scripts/win-assets" / sink_filename(profile)
    shutil.copyfile(sink, share / sink.name)
    receipt = {"schema": "bridgevm.guest-input-live-receipt.v1", "tier": "d5-guest-input",
               "job_id": args.job_id, "commit": head, "profile": profile,
               "input_manifest_sha256": manifest_hash, "input_hashes": hashes,
               "sink_sha256": digest(sink), "claim_eligible": False,
               "criterion_pass": False, "production_ui_proven": False,
               "guest_application_observed": False, "clean_shutdown": False, "production_driver_observed": False,
               "source_integrity": False, "complete": False}
    command = ["bash", str(root / "scripts/run-hvf-windows-installed-boot.sh"),
               "--target", str(clones["image"]), "--vars", str(clones["vars"]),
               "--firmware-code", str(paths["firmware"]), "--evidence-dir", str(boot),
               "--release", "--skip-build", "--watchdog-ms", "450000", "--max-reboots", "0",
               "--ram-mib", "4096", "--smp-cpus", "4", "--max-exits", "50000000",
               "--ramfb-samples", "1000,30000,60000,90000,120000",
               "--display-export-ppm", str(boot / "latest.ppm"), "--no-guest-disk-harvest",
               "--agent-service-control", str(control), "--agent-share-host", str(share),
               "--agent-share-guest", r"C:\BridgeVM\input-proof", "--agent-share-ms", "500",
               "--agent-share-max-kb", "8192"]
    env = {key: value for key, value in os.environ.items() if not key.startswith("BRIDGEVM")}
    env["BRIDGEVM_PREBUILT_PROBE"] = str(paths["binary"])
    process = None
    for sig in (signal.SIGTERM, signal.SIGINT):
        signal.signal(sig, interrupted)
    try:
        subprocess.run(["codesign", "--verify", "--strict", str(paths["binary"])], check=True, timeout=30)
        with (work / "launcher.log").open("xb") as log:
            process = subprocess.Popen(command, cwd=root, env=env, stdout=log,
                                       stderr=subprocess.STDOUT, start_new_session=True)
            driver = make_controller(control, boot / "run.log", share, paths, profile)
            deadline = time.monotonic() + 300
            while not any(line.startswith("BVAGENT SERVICE start") for line in driver.lines()):
                if process.poll() is not None or time.monotonic() >= deadline:
                    raise TimeoutError("installed guest service unavailable")
                time.sleep(0.25)
            driver.deadline = time.monotonic() + 120
            result = driver.run()
            receipt["guest_application_observed"] = result["guest_application_observed"]
            receipt["production_driver_observed"] = getattr(driver, "driver_observed", False)
            if "coherence" in result: receipt["coherence"] = result["coherence"]
            receipt["nonce"] = result["nonce"]
            with control.open("ab", buffering=0) as stream:
                stream.write(b"shutdown /s /t 0\n")
            status = process.wait(timeout=60)
            receipt["process_exit"] = status
            receipt["clean_shutdown"] = status == 0 and any(
                line.startswith("stop: PSCI ") and "(system off)" in line for line in driver.lines())
            if not receipt["clean_shutdown"]:
                raise ValueError("clean guest shutdown not observed")
    except (OSError, ValueError, TimeoutError, subprocess.SubprocessError) as error:
        receipt["failure_type"] = type(error).__name__
    finally:
        for sig in (signal.SIGTERM, signal.SIGINT):
            signal.signal(sig, signal.SIG_IGN)
        finalize(receipt, process, paths, hashes, clones, work)
    print(json.dumps(receipt, sort_keys=True))
    return 0 if all(receipt[key] for key in ("complete", "cleanup_complete", "source_integrity", "clean_shutdown")) and observation_succeeded(profile, receipt) else 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("commit", "job-id", "input-manifest"):
        parser.add_argument("--" + name, required=True)
    try:
        return execute(parser.parse_args())
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        print(json.dumps({"claim_eligible": False, "complete": False, "failure_type": type(error).__name__}))
        return 1


if __name__ == "__main__":
    sys.exit(main())
