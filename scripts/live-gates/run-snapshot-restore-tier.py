#!/usr/bin/env python3
"""Run A19 item 5 from sealed release inputs, leaving a path-free receipt."""
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import subprocess
import sys
from snapshot_restore_inputs import prepare


def main():
    out, job_id, manifest, binary = sys.argv[1:]
    out = Path(out)
    repo = Path(__file__).resolve().parents[2]
    commit = subprocess.check_output(["git", "-C", str(repo), "rev-parse", "HEAD"], text=True).strip()
    receipt = {"tier": "t1-restore-boot", "job_id": job_id, "commit": commit,
               "pass": False, "outcome": "input-preflight-failed", "sample_count": 1}
    status = 1
    try:
        staged = out / "prepared-inputs"
        receipt.update(prepare(manifest, binary, commit, staged))
        environment = dict(os.environ, OUT=str(out), DISK=str(staged / "disk.raw"),
                           VARS=str(staged / "vars.fd"), BRIDGEVM_PREBUILT_PROBE=binary)
        receipt["outcome"] = "failed"
        status = subprocess.run([str(repo / "scripts/verify-snapshot-restore-boots.sh")],
                                cwd=repo, env=environment).returncode
        receipt.update({"pass": status == 0, "outcome": "completed" if status == 0 else "failed"})
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        print("FAIL: snapshot restore tier: " + str(error), file=sys.stderr)
    finally:
        receipt["finished_at"] = datetime.now(timezone.utc).isoformat()
        (out / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
    return 0 if status == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
