#!/usr/bin/env python3
"""A canceled T17 lane ends the product processes it started and still proves cleanup (r97).

The fake helper runs through LaunchServices like the real one and starts a stand-in
app in its own session, so neither is in the tier's process group. The test signals
that group exactly as the queue worker does on `bridgevm-live cancel`.
"""
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "scripts/live-gates"))
import product_e2e_process_absent as probe  # noqa: E402

tier, manifest, tmp, verify, commit = sys.argv[1:]
out = Path(tmp) / "midlane-cancel-out"
log = open(Path(tmp) / "midlane-cancel-tier.log", "wb")
tier_process = subprocess.Popen([tier, "--out", str(out), "--input-manifest", manifest,
                                 "--job-id", "midlane-cancel-fixture"], start_new_session=True,
                                stdout=log, stderr=subprocess.STDOUT)
deadline, marker = time.monotonic() + 60, None
while marker is None and time.monotonic() < deadline:
    assert tier_process.poll() is None, "tier ended before the product app started"
    found = list((out / "private").glob("bridgevm-e2e-midlane-cancel-fixture.*/lane-1/product-app.pid"))
    if found and found[0].read_text().endswith("\n"):
        marker = found[0]
    else:
        time.sleep(0.1)
assert marker is not None, "the fake product app never started"
work, app = marker.parents[1], int(marker.read_text())
assert os.getpgid(app) != tier_process.pid
# An operator process naming the job's private folder, outside the lane tree, must survive.
decoy = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(60)", str(work.parent / "operator-note")],
                         start_new_session=True)
try:
    os.killpg(tier_process.pid, signal.SIGTERM)
    status = tier_process.wait(timeout=90)
    assert status != 0, (status, (out.parent / "midlane-cancel-tier.log").read_text())
    receipt = json.loads((out / "receipt.json").read_text())
    assert receipt["outcome"] == receipt["failure_code"] == "canceled", receipt
    assert receipt["worker_cleanup_verified"] is True and receipt["pass"] is False, receipt
    subprocess.run([verify, str(out / "receipt.json"), "--expected-commit", commit],
                   check=True, stdout=subprocess.DEVNULL)
    assert not os.path.lexists(work)
    assert subprocess.run(["/usr/bin/pgrep", "-f", str(work) + "/"], capture_output=True).returncode == 1
    assert decoy.poll() is None, "a process outside the lane tree was ended"
finally:
    decoy.kill()
    decoy.wait()
    probe.end(str(work) + "/", term_seconds=1.0)  # only matters when the tier left them
