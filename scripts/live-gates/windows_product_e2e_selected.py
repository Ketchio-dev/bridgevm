#!/usr/bin/env python3
"""Hash the media pair the product selects now: `snapshot_pair_cli digest` under the pair lease.

After a restore the original disk and vars paths keep their old bytes and the selected
generation lives under the managed root (crates/bridgevm-hvf/src/managed_pair.rs). Studio
T17 r80 completed every journey stage and was refused because its final media were hashed
at the original paths. The helper is the sealed app bundle's own binary.
"""
from __future__ import annotations
import re, subprocess
from pathlib import Path

CLI = "Contents/Resources/target/release/examples/snapshot_pair_cli"
FIELDS = ["disk_bytes", "disk_sha256", "vars_bytes", "vars_sha256"]

def selected_digests(request: dict) -> tuple[str, str]:
    cli = Path(request["app_bundle_path"]) / CLI
    if not cli.is_file() or cli.is_symlink():
        raise ValueError("product snapshot helper is missing or unsafe")
    done = subprocess.run([str(cli), "digest", request["disk_path"], request["vars_path"]], capture_output=True,
                          text=True, timeout=1800, env={"PATH": "/usr/bin:/bin:/usr/sbin:/sbin", "LANG": "C"})
    pairs = [line.split(" ", 1) for line in done.stdout.splitlines()]
    if done.returncode != 0 or any(len(pair) != 2 for pair in pairs) or [pair[0] for pair in pairs] != FIELDS:
        raise ValueError("selected media digest failed")
    values = dict(pairs)
    if not all(re.fullmatch(r"[0-9a-f]{64}", values[field]) for field in ("disk_sha256", "vars_sha256")):
        raise ValueError("selected media digest is malformed")
    return values["disk_sha256"], values["vars_sha256"]
