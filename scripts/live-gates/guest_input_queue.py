#!/usr/bin/env python3
"""D5-only bridge between queue seals and private installed-input execution."""
import json
from pathlib import Path
import subprocess
import sys
from guest_input_driver_variant import bind
from guest_input_live_inputs import digest, load
from guest_input_protocol import regular_bytes
import guest_input_queue_receipt as receipts


def manifest(path):
    value = json.loads(regular_bytes(path, 16384))
    if not isinstance(value, dict) or value.get("schema") != "bridgevm.guest-input-live.v1":
        raise ValueError("invalid D5 manifest")
    return value


def effective(directory, root, commit, source, binary):
    identity = receipts.job(directory, commit)
    if source.resolve() != (directory / "input-manifest.tsv").resolve():
        raise ValueError("manifest is not queue-owned")
    if digest(source) != identity["input_manifest_sha256"]:
        raise ValueError("queue manifest seal mismatch")
    if binary.resolve() != (directory / "hvf_gic_boot_probe").resolve():
        raise ValueError("binary is not queue-owned")
    if digest(binary) != identity["sealed_binary_sha256"]:
        raise ValueError("queue binary seal mismatch")
    value = manifest(source)
    if value["assets"]["binary"]["sha256"] != identity["sealed_binary_sha256"]:
        raise ValueError("manifest binary identity differs from queue")
    value["assets"]["binary"]["path"] = str(binary.resolve())
    value["assets"]["firmware"]["path"] = str((root / "crates/bridgevm-hvf/firmware/edk2-aarch64-secure-code.fd").resolve())
    bind(directory, value); target = directory / "effective-inputs.json"
    receipts.write_new(target, value)
    load(target, root)  # Validate all assets, including checkout-pinned firmware.
    return identity, target


def run(directory, root, commit, source, binary):
    identity, target = effective(directory, root, commit, source, binary)
    process = subprocess.Popen([sys.executable, str(root / "scripts/live-gates/run-guest-input-live.py"),
                                "--commit", commit, "--job-id", identity["job_id"],
                                "--input-manifest", str(target)])
    try:
        status = process.wait(timeout=600)
    except subprocess.TimeoutExpired:
        process.terminate()
        try:
            process.wait(timeout=30)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=5)
        status = 124
    result = receipts.empty(identity, "incomplete")
    private = (Path.home() / "BridgeVM/work" / ("guest-input-live-" + identity["job_id"]) / "receipt.json")
    try:
        raw = json.loads(regular_bytes(private, 16384))
        if (raw.get("schema") != "bridgevm.guest-input-live-receipt.v1"
                or raw.get("commit") != commit or raw.get("job_id") != identity["job_id"]
                or raw.get("input_manifest_sha256") != digest(target)
                or raw.get("claim_eligible") is not False or raw.get("production_ui_proven") is not False
                or digest(source) != identity["input_manifest_sha256"]
                or digest(binary) != identity["sealed_binary_sha256"]):
            raise ValueError("private diagnostic identity mismatch")
        if all(type(raw.get(key)) is bool for key in receipts.BOOLS):
            result.update({key: raw[key] for key in receipts.BOOLS})
            result["guest_application_observed"] = status == 0 and all(raw[key] for key in receipts.BOOLS)
            result["reason"] = "collected"
    except (OSError, ValueError):
        pass
    receipts.write_new(directory / "receipt.json", receipts.checked(result, identity))
    return 1  # D5 is diagnostic, never a passing release gate.


def main():
    mode, *args = sys.argv[1:]
    if mode in ("binary-path", "binary-hash"):
        entry = manifest(Path(args[0]))["assets"]["binary"]
        value = entry["path" if mode == "binary-path" else "sha256"]
        if not isinstance(value, str) or any(char in value for char in "\r\n\t"):
            raise ValueError("invalid binary metadata")
        print(value)
        return 0
    if mode == "run":
        directory, root, commit, source, binary = args
        return run(Path(directory), Path(root), commit, Path(source), Path(binary))
    if mode in ("finalize", "publish"):
        getattr(receipts, mode)(Path(args[0]), args[1])
        return 0
    raise ValueError("unknown D5 queue action")


if __name__ == "__main__":
    sys.exit(main())
