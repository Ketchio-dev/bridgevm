#!/usr/bin/env python3
"""Actual worker group cancellation with a dummy guest; no Windows runtime proof."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("runner", ROOT / "scripts/live-gates/windows-media-comparison-runner.py")
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


def child(root):
    assets = {key: {"path": str(root / key), "sha256": runner.manifest.seal(root / key)}
              for key in runner.manifest.ASSETS}
    real_run = subprocess.run

    def execute(command, **kwargs):
        if command[0] == "bash" and command[1].endswith("verify-windows-closure-binary.sh"):
            return subprocess.CompletedProcess(command, 0)
        return real_run(command, **kwargs)

    def proof(repo, out, disk, variables, verified):
        return [sys.executable, __file__, "--guest", str(root), str(disk)]

    args = argparse.Namespace(out=root / "diagnostic", input_manifest=root / "inputs.tsv",
                              sealed_binary=root / "binary", job_id="cancel-fixture")
    with patch.object(runner.manifest, "verify", return_value={"assets": assets}), \
            patch.object(runner, "proof_command", side_effect=proof), \
            patch.object(runner.subprocess, "run", side_effect=execute):
        return runner.run(args)


def test():
    for state, expected in (("", 1), ("Z", 1), ("S", 0)):
        probe = 'source "$1/scripts/live-gates/live-process-cleanup.sh"; kill() { return 0; }; ps() { printf "%s" "$2"; }; bridgevm_process_alive 99999'
        probe = probe.replace('printf "%s" "$2"', f'printf "%s" "{state}"')
        assert subprocess.run(["bash", "-c", probe, "state-test", str(ROOT)]).returncode == expected
    with tempfile.TemporaryDirectory(prefix="media cancellation ") as temporary:
        root = Path(temporary)
        for key in runner.manifest.ASSETS:
            (root / key).write_bytes(key.encode())
            (root / key).chmod(0o400)
        manifest = root / "inputs.tsv"
        manifest.write_text("fixture")
        commit = subprocess.check_output(["git", "-C", str(ROOT), "rev-parse", "HEAD"], text=True).strip()
        digest = hashlib.sha256(manifest.read_bytes()).hexdigest()
        (root / "job.env").write_text(f"job_id=cancel-fixture\ntier=d1-windows-media-comparison\ncommit={commit}\ninput_manifest_sha256={digest}\n")
        script = '''set -euo pipefail
set -m
source "$1/scripts/live-gates/live-process-cleanup.sh"
"$2" "$3" --child "$4" &
tier_pid=$!
trap 'bridgevm_terminate_process_group_bounded "$tier_pid" || true' EXIT
ready=0
for ((i=0; i<100; i++)); do
  if [[ -f "$4/ready" ]]; then ready=1; break; fi
  kill -0 "$tier_pid" || exit 1
  sleep 0.05
done
[[ "$ready" == 1 ]]
touch "$4/cancel.requested"
if bridgevm_wait_for_tier_group "$tier_pid" "$4/cancel.requested" cancel-fixture; then :; else exit 1; fi
! bridgevm_process_group_alive "$tier_pid"
trap - EXIT
'''
        subprocess.run(["bash", "-c", script, "cancel-test", str(ROOT), sys.executable,
                        __file__, str(root)], check=True, timeout=25)
        guest_pid, disk_name = (root / "ready").read_text().splitlines()
        state = subprocess.run(["ps", "-o", "state=", "-p", guest_pid], capture_output=True, text=True).stdout.strip()
        assert not state or state.startswith("Z"), "guest remains alive"
        disk = Path(disk_name)
        assert disk.is_file(), "active diagnostic scratch was removed on cancellation"
        assert disk.read_bytes() == b"original"
        assert not (root / "diagnostic/reinjected").exists(), "second lane ran after cancel"
        for key in runner.manifest.ASSETS:
            assert (root / key).read_bytes() == key.encode()
            assert (root / key).stat().st_mode & 0o777 == 0o400
        subprocess.run([str(ROOT / "scripts/live-gates/write-missing-receipt.sh"),
                        "d1-windows-media-comparison", str(root), str(ROOT), "cancel-fixture", commit], check=True)
        subprocess.run([str(ROOT / "scripts/live-gates/publish-receipt.sh"),
                        "d1-windows-media-comparison", str(root), str(ROOT), commit], check=True)
        public = json.loads((root / "receipt.public.json").read_text())
        assert public["outcome"] == "canceled" and public["pass"] is public["claim_eligible"] is False
        stale = runner.manifest.seal(root / "original")
        (root / "original").chmod(0o600)
        (root / "original").write_bytes(b"changed after seal")
        work = root / "changed-clone"
        work.mkdir()
        try:
            runner.clone_media(root / "original", root / "vars", work, stale, runner.manifest.seal(root / "vars"))
            raise AssertionError("pre-copy source mutation accepted")
        except ValueError:
            pass
    print("PASS: actual worker-group cancellation, scratch retention and pre-copy mutation refusal")


if __name__ == "__main__":
    if len(sys.argv) > 1 and sys.argv[1] == "--child":
        raise SystemExit(child(Path(sys.argv[2])))
    if len(sys.argv) > 1 and sys.argv[1] == "--guest":
        with Path(sys.argv[3]).open("rb") as held_disk:
            (Path(sys.argv[2]) / "ready").write_text(f"{os.getpid()}\n{sys.argv[3]}\n")
            time.sleep(20)
    else:
        test()
