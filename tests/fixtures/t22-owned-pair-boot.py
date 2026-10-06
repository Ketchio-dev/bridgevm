#!/usr/bin/env python3
"""Owned host-process fixture; never starts a VM or queries Windows."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "integration"))
from t22_pair_fixtures import report

parser = argparse.ArgumentParser()
for key in ("target", "vars", "evidence-dir", "agent-service-control", "agent-share-host"):
    parser.add_argument("--" + key, required=True)
args, other = parser.parse_known_args()
boot, share, control = Path(args.evidence_dir), Path(args.agent_share_host), Path(args.agent_service_control)
boot.mkdir(mode=0o700)
mode = os.environ.get("T22_FIXTURE_MODE", "normal")
log = boot / "run.log"
with log.open("xb", buffering=0) as stream:
    def line(text): stream.write((text + "\n").encode())
    if mode != "no-service": line("BVAGENT SERVICE start")
    position, deadline = 0, time.monotonic() + 20
    while time.monotonic() < deadline:
        raw = control.read_bytes()
        complete = raw.rfind(b"\n") + 1
        commands = raw[position:complete].decode().splitlines(); position = complete
        for command in commands:
            if command == "shutdown.exe /s /t 0":
                with Path(args.vars).open("r+b") as variables:
                    variables.write(b"owned synthetic boot variables")
                line("=== EDK2 boot probe (with Apple hv_gic) ===")
                line("stop: PSCI 0x84000008 (system off)" if mode != "bad-stop" else "stop: watchdog")
                line("host media: NVMe disk written back: owned.raw (67108864 bytes)")
                line("serial raw bytes: 00000000 output bytes: 00000000")
                line("--- serial (tail) ---")
                line(""); line("--- end ---")
                sys.exit(0)
            line("BVAGENT CMD " + command + " exit=0")
            if "BVINPUT_STAGED " in command:
                token = re.search(r"BVINPUT_STAGED ([0-9a-f-]+)", command)[1]
                line("BVINPUT_STAGED " + token)
            elif " -Action Launch " in command:
                nonce = re.search(r"-Nonce ([0-9a-f]{32})", command)[1]
                digest = re.search(r"-ExpectedSha256 ([0-9a-f]{64})", command)[1]
                value = report(nonce, digest)
                if mode == "encrypted": value["volumes"][1]["encryption_method"] = 6
                data = json.dumps(value, separators=(",", ":")).encode()
                (share / f"result-{nonce}.json").write_bytes(data)
                (share / f"result-{nonce}.done").write_bytes(hashlib.sha256(data).hexdigest().encode())
                line("T22-PAIR-QUERY-LAUNCHED-" + nonce)
            else:
                raise RuntimeError("unexpected fixture command")
            line("BVAGENT END " + command)
        time.sleep(.01)
raise TimeoutError("owned fixture received no natural shutdown")
