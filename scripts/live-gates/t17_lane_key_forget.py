#!/usr/bin/env python3
"""Forget each T17 lane's vTPM key through the sealed app before cleanup removes the lane.

The product creates one Keychain key per VM, named by the lane VM slug, the
first time that VM starts with empty vTPM state, and stopping the app leaves it
in place. The only product delete that proves which key it removed is
`--vtpm-lifecycle forget-import`: it opens a recovery package with its code,
checks the state fingerprint and deletes only a stored key equal to the
packaged one. Each lane therefore exports its own slug's key into a new private
folder inside the lane and forgets it with the same stable ID, state, package
and code. The package and code are removed again whatever the outcome; what
cannot be removed is named, and a signal that kills this process leaves them.

A lane whose state is empty or absent can still own a key, because the key
exists before swtpm writes state. Only an export refusal carrying the product's
missing-key message shows that such a lane has none; with state present that
refusal is a failure. Every lane is checked before the app runs, and one
invalid lane runs nothing. Each valid lane is then tried even when another
refuses, and every refusal is reported without private paths. Any refusal
exits 1, so the tier reports cleanup-failed and keeps the tree. A second run is
not idempotent: it refuses each lane already forgotten whose state holds data,
because that state's key is then missing.
"""
from __future__ import annotations

import argparse
import errno
import hashlib
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
import tempfile

sys.path.insert(0, str(Path(__file__).resolve().parent))
import product_e2e_work as WORK  # noqa: E402
from product_e2e_identity import is_sha256  # noqa: E402
from t17_lane_request import Refusal, bounded_json, lane_request, lane_state  # noqa: E402

# VTPMStateSecurityError.missingKeyForExistingState as VTPMLifecycleCommand prints it.
MISSING_KEY = ("ERROR: 기존 vTPM 상태의 Keychain 키가 없습니다. 상태를 덮어쓰지 않았습니다. "
               "복구 키를 복원하거나 명시적으로 TPM을 재설정하세요.")
FORGOT = "forgot_imported_key="
PACKAGE, CODE, SCRATCH = "vtpm-recovery.json", "vtpm-recovery-code.txt", "empty-state"
LANE = re.compile(r"lane-([1-3])\Z")
WORK_NAME = re.compile(r"bridgevm-e2e-" + WORK.JOB + r"\.[A-Za-z0-9]{6}\Z")
DIRECTORY = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC
TIMEOUT_SECONDS = 120
OUTPUT_LIMIT = 65_536


def describe(error: BaseException) -> str:
    """A Refusal's own text; any other error only by class and errno name, so no path reaches the tier log."""
    if isinstance(error, Refusal):
        return str(error)
    name = errno.errorcode.get(error.errno) if isinstance(error, OSError) and error.errno else None
    return f"{type(error).__name__} {name}" if name else type(error).__name__


def sealed_app(verified: Path) -> tuple[str, Path, Path, str]:
    """Return the verified campaign mode, app bundle, app executable and its sealed SHA-256."""
    value = bounded_json(verified, "verified inputs")
    try:
        assets = value["assets"]
        mode, bundle = value["campaign_mode"], Path(assets["app_bundle"]["path"])
        executable, digest = Path(assets["app_executable"]["path"]), assets["app_executable"]["sha256"]
    except (KeyError, TypeError) as error:
        raise Refusal("verified inputs lack the sealed app executable") from error
    if (value.get("verified") is not True or mode not in ("pilot", "release") or not is_sha256(digest)
            or not bundle.is_absolute() or executable != bundle / "Contents/MacOS/BridgeVMControl"):
        raise Refusal("verified inputs do not seal a packaged app executable")
    return mode, bundle, executable, digest


def require_sealed(executable: Path, digest: str) -> None:
    descriptor = os.open(executable, os.O_RDONLY | os.O_NOFOLLOW | os.O_CLOEXEC)
    try:
        if not stat.S_ISREG(os.fstat(descriptor).st_mode):
            raise Refusal("sealed app executable is not a regular file")
        hasher = hashlib.sha256()
        for chunk in iter(lambda: os.read(descriptor, 1 << 20), b""):
            hasher.update(chunk)
    finally:
        os.close(descriptor)
    if hasher.hexdigest() != digest:
        raise Refusal("app executable differs from its sealed SHA-256")


def run(executable: Path, digest: str, arguments: list[str]) -> subprocess.CompletedProcess:
    require_sealed(executable, digest)
    try:
        completed = subprocess.run([str(executable), "--vtpm-lifecycle", *arguments], stdin=subprocess.DEVNULL,
                                   capture_output=True, timeout=TIMEOUT_SECONDS, check=False)
    except subprocess.TimeoutExpired as error:
        raise Refusal(f"packaged vTPM {arguments[0]} did not finish within {TIMEOUT_SECONDS} s") from error
    if len(completed.stdout) > OUTPUT_LIMIT or len(completed.stderr) > OUTPUT_LIMIT:
        raise Refusal(f"packaged vTPM {arguments[0]} output exceeds {OUTPUT_LIMIT} bytes")
    return completed


def discard(folder: Path, descriptor: int) -> list[str]:
    """Remove the recovery package and code and any scratch state, then the emptied folder; return what stayed."""
    left = []
    for name, remove in ((PACKAGE, os.unlink), (CODE, os.unlink), (SCRATCH, os.rmdir)):
        try:
            remove(name, dir_fd=descriptor)
        except FileNotFoundError:
            pass
        except OSError as error:
            left.append(f"{name} ({describe(error)})")
    if not left:
        try:
            os.rmdir(folder)
        except OSError as error:
            left.append(f"{folder.name} ({describe(error)})")
    return left


def forget(executable: Path, digest: str, request: dict, ordinal: int, state: Path | None, holds_data: bool) -> None:
    folder = Path(tempfile.mkdtemp(prefix="vtpm-key-forget.", dir=request["lane_root"]))
    descriptor = os.open(folder, DIRECTORY)
    try:
        if state is None:
            state = folder / SCRATCH
            state.mkdir(mode=0o700)
        slug, package, code = request["vm_slug"], folder / PACKAGE, folder / CODE
        identity = ["--stable-vm-id", slug, "--state-dir", str(state), "--package", str(package), "--recovery-code-file", str(code)]
        exported = run(executable, digest, ["export", *identity])
        if exported.returncode != 0:
            missing = exported.returncode == 1 and MISSING_KEY in exported.stderr.decode("utf-8", "replace").splitlines()
            if holds_data or not missing or os.path.lexists(package) or os.path.lexists(code):
                raise Refusal("key was not exported" + (" although its vTPM state holds data" if holds_data
                              else " and the product did not report it missing"))
            return
        forgotten = run(executable, digest, ["forget-import", *identity])
        lines = [line for line in forgotten.stdout.decode("utf-8", "replace").splitlines() if line.startswith(FORGOT)]
        if forgotten.returncode != 0 or lines != [FORGOT + slug]:
            raise Refusal("key was not forgotten: forget-import gave no exact proof for its slug")
    finally:
        for item in discard(folder, descriptor):
            print(f"T17 lane vTPM key forget could not remove lane {ordinal} {item}", file=sys.stderr)
        os.close(descriptor)


def forget_lanes(work: Path, verified: Path) -> list[str]:
    """Check every lane before the app runs, then try each valid lane; return every lane's refusal."""
    mode, bundle, executable, digest = sealed_app(verified)
    require_sealed(executable, digest)
    try:
        WORK.directory(work)
    except ValueError as error:
        raise Refusal("work tree is not a private canonical directory") from error
    if not WORK_NAME.match(work.name):
        raise Refusal("work tree is not a T17 job tree")
    lanes = []
    for name in sorted(os.listdir(work)):
        match = LANE.match(name)
        if not match or int(match.group(1)) > (3 if mode == "release" else 1) or not stat.S_ISDIR(os.lstat(work / name).st_mode):
            raise Refusal(f"work tree holds an entry that is not a {mode} lane: {name!r}")
        ordinal = int(match.group(1))
        try:
            request = lane_request(work, work / name, ordinal, mode, bundle, executable)
            if request is not None:
                lanes.append((request, ordinal, *lane_state(request, ordinal)))
        except OSError as error:
            raise Refusal(f"lane {ordinal} could not be read ({describe(error)})") from error
    refusals = []
    for request, ordinal, state, holds_data in lanes:
        try:
            forget(executable, digest, request, ordinal, state, holds_data)
        except (OSError, ValueError, subprocess.SubprocessError) as error:
            refusals.append(f"lane {ordinal}: {describe(error)}")
    return refusals


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Forget each T17 lane's vTPM key before its tree is removed.")
    parser.add_argument("--work", type=Path, required=True)
    parser.add_argument("--verified", type=Path, required=True)
    args = parser.parse_args(argv)
    try:
        refusals = forget_lanes(args.work, args.verified)
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        refusals = [describe(error)]
    for refusal in refusals:
        print(f"T17 lane vTPM key forget refused: {refusal}", file=sys.stderr)
    return 1 if refusals else 0


if __name__ == "__main__":
    raise SystemExit(main())
