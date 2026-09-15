#!/usr/bin/env python3
"""Exercise the ordinary native CLI entry point with owned, read-only fixtures."""
import hashlib
import json
import os
from pathlib import Path
import signal
import stat
import subprocess
import sys
import tempfile


def run(binary, arguments, expected):
    process = subprocess.Popen(
        [str(binary), "--cli", *arguments], stdout=subprocess.PIPE,
        stderr=subprocess.PIPE, start_new_session=True,
    )
    try:
        stdout, stderr = process.communicate(timeout=10)
    except subprocess.TimeoutExpired:
        os.killpg(process.pid, signal.SIGTERM)
        try:
            process.communicate(timeout=1)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            process.communicate(timeout=1)
        raise AssertionError("Native CLI did not exit within 10 seconds") from None
    assert process.returncode == expected, (arguments, process.returncode, stderr.decode(errors="replace"))
    return stdout, stderr


def tree(root):
    return {str(path.relative_to(root)): hashlib.sha256(path.read_bytes()).hexdigest()
            if path.is_file() else "directory" for path in sorted(root.rglob("*"))}


def main():
    assert len(sys.argv) == 2, "usage: native-app-cli-contract.py ABSOLUTE_BINARY"
    binary = Path(sys.argv[1])
    assert binary.is_absolute() and not binary.is_symlink()
    metadata = binary.stat()
    assert stat.S_ISREG(metadata.st_mode) and metadata.st_size <= 256 * 1024 * 1024
    code = binary.read_bytes()
    assert code[:4] in (b"\xcf\xfa\xed\xfe", b"\xfe\xed\xfa\xcf", b"\xca\xfe\xba\xbe")
    assert b"bridgevm-native-cli-v1" in code, "Older app refused without execution"
    with tempfile.TemporaryDirectory(prefix="bridgevm-native-cli-contract-") as temporary:
        root = Path(temporary)
        library = root / "vms"
        output, error = run(binary, ["--help"], 0)
        assert b"bridgevm-native-cli-v1" in output and not error
        output, error = run(binary, ["list", "--library", str(library), "--json"], 0)
        absent = json.loads(output)
        assert absent["schema"] == "bridgevm.app-library.v1"
        assert absent["complete"] and absent["records"] == [] and not error
        assert not library.exists(), "Read-only query created the library"
        entry = library / "개발-vm"
        entry.mkdir(parents=True)
        configuration = {
            "id": "../../wrong-id", "name": "개발 VM", "displayName": "개발 VM",
            "backendKind": "hvf-engine", "bundlePath": str(entry),
            "runnerPath": "", "launchSpecPath": "", "handoffPath": "",
            "sshKeyPath": "private-key-path-must-not-be-rendered", "sshUser": "fixture-user",
            "leasesPath": "", "guestName": "", "displayWidth": 1280, "displayHeight": 800,
            "memMiB": 6144, "cpuCount": 4, "installPending": False,
        }
        (entry / "vm.json").write_text(json.dumps(configuration), encoding="utf-8")
        before = tree(root)
        output, error = run(binary, ["inspect", "개발-vm", "--library", str(library), "--json"], 0)
        snapshot = json.loads(output)
        assert snapshot["complete"] and len(snapshot["records"]) == 1 and not error
        record = snapshot["records"][0]
        assert record["id"] == "개발-vm" and record["runtimeState"] == "unobserved"
        assert record["cpuCount"] == 4 and record["memoryMiB"] == 6144
        assert b"private-key-path-must-not-be-rendered" not in output
        repeated, repeated_error = run(binary, ["inspect", "개발-vm", "--library", str(library), "--json"], 0)
        assert repeated == output and not repeated_error, "Unchanged inventory JSON is not stable"
        output, error = run(binary, ["readiness", "개발-vm", "--library", str(library), "--json"], 1)
        readiness = json.loads(output)
        assert readiness["schema"] == "bridgevm.app-readiness.v1" and not error
        assert not readiness["launchReady"] and readiness["launchBlockers"]
        assert all(issue["scope"] == "launch" for issue in readiness["launchBlockers"])
        assert all(issue["scope"] == "release" for issue in readiness["releaseBlockers"])
        output, error = run(binary, ["start", "개발-vm", "--library", str(library)], 2)
        assert not output and error
        run(binary, ["inspect", "missing", "--library", str(library), "--json"], 1)
        assert tree(root) == before, "Native queries changed input files or created runtime state"
        corrupt = library / "corrupt"
        corrupt.mkdir()
        (corrupt / "vm.json").write_text("broken configuration", encoding="utf-8")
        before = tree(root)
        output, error = run(binary, ["list", "--library", str(library), "--json"], 1)
        snapshot = json.loads(output)
        assert not snapshot["complete"] and len(snapshot["records"]) == 1
        assert len(snapshot["issues"]) == 1 and not error
        assert tree(root) == before
    print("native app CLI: PASS (8 real CLI processes, owned fixtures unchanged; no GUI or VM requested)")


if __name__ == "__main__":
    main()
