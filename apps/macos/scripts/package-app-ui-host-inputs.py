#!/usr/bin/env python3
"""Package and sign fresh app-only diagnostic inputs before they are sealed."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import plistlib
import re
import stat
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "scripts/live-gates"))
from app_ui_host_bundle import bundle_metadata
from app_ui_host_v2_bundle import driver_bundle_metadata
from app_ui_host_manifest import verify_executable

RESOURCES = ("windows-boot-seed-vars.fd.gz",
             "secureboot-microsoft-windows-transition-aarch64-v1.6.5.json")


def canonical(path, existing=True):
    raw = str(path)
    path = Path(path)
    if not path.is_absolute() or raw != str(path) or raw != os.path.normpath(raw):
        raise ValueError("absolute canonical path required")
    if path.resolve(strict=existing) != path:
        raise ValueError("symlink ancestor refused")
    return path


def regular_bytes(path, limit):
    path = canonical(path)
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    try:
        before = os.fstat(fd)
        if not stat.S_ISREG(before.st_mode) or not 0 < before.st_size <= limit:
            raise ValueError("bounded regular file required")
        chunks, total = [], 0
        while True:
            part = os.read(fd, min(1024 * 1024, limit + 1 - total))
            if not part:
                break
            chunks.append(part)
            total += len(part)
            if total > limit:
                raise ValueError("input exceeds its bound")
        after = os.fstat(fd)
        if (before.st_dev, before.st_ino, before.st_size, before.st_mtime_ns, before.st_ctime_ns) != (
                after.st_dev, after.st_ino, after.st_size, after.st_mtime_ns, after.st_ctime_ns):
            raise ValueError("input changed while being read")
        if total != before.st_size:
            raise ValueError("input length changed")
        return b"".join(chunks)
    finally:
        os.close(fd)


def publish(path, data, mode=0o400):
    missing = []
    current = path.parent
    while not current.exists():
        missing.append(current)
        current = current.parent
    for parent in reversed(missing):
        parent.mkdir(mode=0o700)
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, mode)
    try:
        with os.fdopen(fd, "wb", closefd=False) as target:
            target.write(data)
            target.flush()
            os.fsync(fd)
    finally:
        os.close(fd)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def code(command):
    result = subprocess.run(["/usr/bin/codesign", *command], capture_output=True, text=True, timeout=20)
    if result.returncode:
        raise ValueError("codesign refused: " + result.stderr.strip())
    return {"arguments": command, "exit_code": result.returncode,
            "stdout": result.stdout, "stderr": result.stderr}


def sign(bundle, identifier):
    signed = code(["--force", "--sign", "-", "--identifier", identifier,
                   "--timestamp=none", str(bundle)])
    for path in bundle.rglob("*"):
        if path.is_dir():
            path.chmod(0o700)
        elif path.is_file():
            path.chmod(0o500 if path.parent.name == "MacOS" else 0o400)
    verified = code(["--verify", "--strict", "--verbose=4", str(bundle)])
    display = code(["--display", "--verbose=4", "--requirements", "-", str(bundle)])
    return {"sign": signed, "verify": verified, "display": display}


def package(host_input, launcher_input, resource_commit, output):
    if not re.fullmatch(r"[0-9a-f]{40}", resource_commit):
        raise ValueError("full resource commit required")
    output = canonical(output, existing=False)
    canonical(output.parent)
    host_bytes = regular_bytes(host_input, 512 * 1024 * 1024)
    launcher_bytes = regular_bytes(launcher_input, 512 * 1024 * 1024)
    output.mkdir(mode=0o700)
    host = output / "BridgeVMAppUIHost.app"
    driver = output / "BridgeVMAppUIDriver.app"
    host_exe = host / "Contents/MacOS/BridgeVMControl"
    driver_exe = driver / "Contents/MacOS/AppUIHostLauncher"
    publish(host_exe, host_bytes, 0o700)
    publish(driver_exe, launcher_bytes, 0o700)
    publish(host / "Contents/Info.plist", plistlib.dumps(bundle_metadata("dev.bridgevm.app-ui-host")))
    publish(driver / "Contents/Info.plist", plistlib.dumps(driver_bundle_metadata()))
    resource_hashes = {}
    for name in RESOURCES:
        relative = "apps/macos/Sources/BridgeVMControl/Resources/" + name
        reference = resource_commit + ":" + relative
        size = int(subprocess.check_output(["git", "-C", str(ROOT), "cat-file", "-s", reference], timeout=5))
        if not 0 < size <= 4 * 1024 * 1024:
            raise ValueError("resource exceeds fixed bound")
        data = subprocess.check_output(["git", "-C", str(ROOT), "cat-file", "blob", reference], timeout=5)
        if len(data) != size:
            raise ValueError("resource length differs")
        publish(host / "Contents/Resources/BridgeVMApp_BridgeVMControl.bundle" / name, data)
        resource_hashes[relative] = digest(data)
    signatures = {"host": sign(host, "dev.bridgevm.app-ui-host"),
                  "driver": sign(driver, "dev.bridgevm.app-ui-driver")}
    bare = output / "AppUIHostLauncher"
    publish(bare, regular_bytes(driver_exe, 512 * 1024 * 1024), 0o500)
    # The bare supervisor has identical code pages, but no bundle resource context.
    # Full resource signatures are verified only on the two complete bundles.
    verify_executable(bare, digest(regular_bytes(driver_exe, 512 * 1024 * 1024)))
    paths = {"binary": host_exe, "launcher": bare, "host_info": host / "Contents/Info.plist",
             "host_resources": host / "Contents/_CodeSignature/CodeResources",
             "driver_info": driver / "Contents/Info.plist",
             "driver_resources": driver / "Contents/_CodeSignature/CodeResources"}
    limits = {"binary": 512 * 1024 * 1024, "launcher": 512 * 1024 * 1024,
              "host_info": 64 * 1024, "driver_info": 64 * 1024,
              "host_resources": 1024 * 1024, "driver_resources": 1024 * 1024}
    artifacts = {name: {"path": str(path), "sha256": digest(regular_bytes(path, limits[name]))}
                 for name, path in paths.items()}
    if artifacts["launcher"]["sha256"] != digest(regular_bytes(driver_exe, 512 * 1024 * 1024)):
        raise ValueError("driver and supervisor executable bytes differ")
    if digest(regular_bytes(host_input, 512 * 1024 * 1024)) != digest(host_bytes) or digest(
            regular_bytes(launcher_input, 512 * 1024 * 1024)) != digest(launcher_bytes):
        raise ValueError("original input was mutated")
    record = {"schema_version": 1, "kind": "app-ui-host-v2-packaging", "resource_commit": resource_commit,
              "artifacts": artifacts, "resource_sha256": resource_hashes, "signatures": signatures,
              "input_sha256": {"host": digest(host_bytes), "launcher": digest(launcher_bytes)},
              "same_driver_supervisor_bytes": True, "original_inputs_unchanged": True,
              "gui_launched": False, "accessibility_trust_observed": False}
    publish(output / "packaging-inputs.json", (json.dumps(record, indent=2, sort_keys=True) + "\n").encode())
    return record


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("host-binary", "launcher-binary", "resource-commit", "output"):
        parser.add_argument("--" + name, required=True)
    args = parser.parse_args()
    record = package(args.host_binary, args.launcher_binary, args.resource_commit, args.output)
    print(json.dumps({"output": args.output, "artifacts": record["artifacts"]}, sort_keys=True))


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        print("app UI packaging refused: " + str(error), file=sys.stderr)
        sys.exit(2)
