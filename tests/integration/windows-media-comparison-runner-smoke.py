#!/usr/bin/env python3
"""Run real APFS clone preparation with a fake proof process, never a guest."""
import argparse
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("comparison", ROOT / "scripts/live-gates/windows-media-comparison-runner.py")
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)
real_run = subprocess.run


def scenario(mode):
    with tempfile.TemporaryDirectory(prefix="media runner ") as temporary:
        root = Path(temporary)
        inputs = root / "inputs.tsv"
        inputs.write_text("fixture")
        assets = {}
        for key in runner.manifest.ASSETS:
            path = root / key
            path.write_bytes(key.encode())
            path.chmod(0o400)
            assets[key] = {"path": str(path), "sha256": runner.manifest.seal(path)}
        commands = []

        def verify(*args):
            for asset in assets.values():
                if runner.manifest.seal(Path(asset["path"])) != asset["sha256"]:
                    raise ValueError("source changed")
            return {"assets": assets}

        def execute(command, **kwargs):
            if command[0] in ("cp", "git"):
                return real_run(command, **kwargs)
            if command[0] == "lsof":
                return subprocess.CompletedProcess(command, 0 if mode == "busy" else 1,
                                                   b"123\n" if mode == "busy" else b"", b"")
            if command[1].endswith("verify-windows-closure-binary.sh"):
                return subprocess.CompletedProcess(command, 0)
            assert command[1].endswith("windows-1.0-closure-interact.sh")
            commands.append((command, kwargs["env"]))
            disk = Path(command[command.index("--target") + 1])
            variables = Path(command[command.index("--vars") + 1])
            assert disk.stat().st_mode & 0o777 == variables.stat().st_mode & 0o777 == 0o600
            assert disk.stat().st_ino != variables.stat().st_ino
            disk.write_bytes(b"guest changes")
            if mode == "changed":
                changed = Path(assets["original"]["path"])
                changed.chmod(0o600)
                changed.write_bytes(b"changed")
            return subprocess.CompletedProcess(command, 0 if mode == "success" else 1)

        args = argparse.Namespace(out=root / "result", input_manifest=inputs,
                                  sealed_binary=root / "binary", job_id="fixture")
        with patch.object(runner.manifest, "verify", side_effect=verify), patch.object(runner.subprocess, "run", side_effect=execute):
            code = runner.run(args)
        result = json.loads((args.out / "receipt.json").read_text())
        assert result["pass"] is result["claim_eligible"] is result["criterion_pass"] is False
        assert result["capability_promotion"] is False and result["passes"] == 0
        if mode in ("success", "failure"):
            assert code == 0 and result["outcome"] == "diagnostic-complete"
            assert [lane["label"] for lane in result["lanes"]] == ["original", "reinjected"]
            assert len(commands) == 2 and commands[0][1] == commands[1][1]
            for option in ("--binary", "--viogpu-dir", "--moltenvk", "--watchdog-ms"):
                assert commands[0][0][commands[0][0].index(option)+1] == commands[1][0][commands[1][0].index(option)+1]
            for asset in assets.values():
                assert Path(asset["path"]).stat().st_mode & 0o777 == 0o400
            assert not list(args.out.glob("original-*/*raw"))
            assert not list(args.out.glob("reinjected-*/*raw"))
        else:
            assert code == 1 and len(commands) == 1
            assert result["outcome"] == "diagnostic-incomplete"
            assert list(args.out.glob("original-*/*raw")), "unsafe lane must retain its clones"


for mode in ("success", "failure", "busy", "changed"):
    scenario(mode)
print("PASS: diagnostic lanes, failed-guest continuation, source mutation and busy-media refusal")
