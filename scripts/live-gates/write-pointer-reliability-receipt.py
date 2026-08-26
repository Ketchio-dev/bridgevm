#!/usr/bin/env python3
"""Write the retained B4 pointer receipt without exposing private paths."""

from __future__ import annotations

import argparse
import json
import platform
import subprocess
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--out", type=Path, required=True)
    for name in ("job-id", "commit", "image", "vars", "manifest", "package", "umd", "landed", "p95", "outcome"):
        parser.add_argument(f"--{name}", required=True)
    parser.add_argument("--pass", dest="passed", action="store_true")
    args = parser.parse_args()
    host = subprocess.run(["sysctl", "-n", "hw.model"], text=True, capture_output=True)
    receipt = {
        "tier": "t8-pointer-reliability", "gate_id": "b4-pointer-click-reliability",
        "criterion": "B4", "job_id": args.job_id, "commit": args.commit,
        "image_sha256": args.image, "vars_sha256": args.vars,
        "input_manifest_sha256": args.manifest, "sealed_package_sha256": args.package,
        "installed_umd_sha256": args.umd, "driver_version": "120.50.0.0",
        "host_model": host.stdout.strip() if host.returncode == 0 else "unavailable",
        "macos_version": platform.mac_ver()[0], "sample_count": 20,
        "landed": args.landed, "p95_first_changed_ms": args.p95,
        "finished_at": subprocess.check_output(["date", "-u", "+%Y-%m-%dT%H:%M:%SZ"], text=True).strip(),
        "outcome": args.outcome, "pass": args.passed,
    }
    args.out.mkdir(parents=True, exist_ok=True)
    (args.out / "receipt.json").write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
