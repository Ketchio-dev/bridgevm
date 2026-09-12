#!/usr/bin/env python3
"""Bind request campaign mode to verified inputs; no VM or real media."""
import json
from pathlib import Path
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[2]
WRITER = ROOT / "scripts/live-gates/make-windows-product-e2e-request.py"
KEYS = "app_bundle app_executable runner firmware secure_boot_policy iso bundled_vars_seed guest_payload guest_payload_manifest".split()
CASES = (("pilot", "pilot", True), ("release", "release", True),
         ("pilot", "release", False), ("release", "pilot", False),
         (None, "pilot", False), ("invalid", "pilot", False))

for verified_mode, requested_mode, accepted in CASES:
    with tempfile.TemporaryDirectory(prefix="bridgevm-e2e-mode-", dir="/tmp") as directory:
        root = Path(directory)
        lane = root / "lane"
        lane.mkdir()
        verified = root / "verified.json"
        value = {"verified": True, "assets": {key: {"path": "/fixture/" + key} for key in KEYS}}
        if verified_mode is not None:
            value["campaign_mode"] = verified_mode
        verified.write_text(json.dumps(value))
        output = lane / "request.json"
        result = subprocess.run([
            sys.executable, str(WRITER), "--out", str(output), "--verified", str(verified),
            "--job-id", "mode-contract", "--commit", "a" * 40, "--mode", requested_mode,
            "--lane", "1", "--nonce", "b" * 64, "--lane-root", str(lane),
        ], capture_output=True, text=True, timeout=10)
        assert (result.returncode == 0) == accepted, (verified_mode, requested_mode, result.stderr)
        assert output.exists() == accepted, "rejected mode must not create a request"
        if accepted:
            assert json.loads(output.read_text())["campaign_mode"] == verified_mode
print(f"PASS: {len(CASES)} request campaign-mode cases; no live VM")
