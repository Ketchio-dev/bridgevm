"""Run only the sealed product's pair helper and validate its digest protocol."""
from __future__ import annotations

import re
import subprocess
from pathlib import Path

CLI = "Contents/Resources/target/release/examples/snapshot_pair_cli"
FIELDS = ["disk_bytes", "disk_sha256", "vars_bytes", "vars_sha256"]


def run(request: dict, *arguments: str) -> str:
    cli = Path(request["app_bundle_path"]) / CLI
    if not cli.is_file() or cli.is_symlink():
        raise ValueError("product snapshot helper is missing or unsafe")
    done = subprocess.run(
        [str(cli), *arguments], capture_output=True, text=True, timeout=1800,
        env={"PATH": "/usr/bin:/bin:/usr/sbin:/sbin", "LANG": "C"},
    )
    if done.returncode != 0:
        raise ValueError("product snapshot helper failed")
    return done.stdout


def selected(request: dict) -> dict[str, str | int]:
    output = run(request, "digest", request["disk_path"], request["vars_path"])
    pairs = [line.split(" ", 1) for line in output.splitlines()]
    if any(len(pair) != 2 for pair in pairs) or [pair[0] for pair in pairs] != FIELDS:
        raise ValueError("selected media digest failed")
    values = dict(pairs)
    if not all(re.fullmatch(r"[0-9a-f]{64}", values[field]) for field in ("disk_sha256", "vars_sha256")):
        raise ValueError("selected media digest is malformed")
    for field in ("disk_bytes", "vars_bytes"):
        raw = values[field]
        if not re.fullmatch(r"[1-9][0-9]{0,19}", raw) or int(raw) > 2**64 - 1:
            raise ValueError("selected media size is malformed")
        values[field] = int(raw)
    return values
