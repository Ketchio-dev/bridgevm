#!/usr/bin/env python3
"""Forget each T17 lane's vTPM key through the sealed app before cleanup removes the lane.

The product creates one Keychain key per VM, named by the lane VM slug, the
first time that VM starts with empty vTPM state, and stopping the app leaves it
in place. The only product delete that proves which key it removed is
`--vtpm-lifecycle forget-import`: it opens a recovery package with its code,
checks the state fingerprint and deletes only a stored key equal to the
packaged one. Each lane therefore exports its own slug's key into a new private
folder inside the lane and forgets it with the same stable ID, state, package
and code.

A lane whose state is empty or absent can still own a key, because the key
exists before swtpm writes state. Only an export refusal carrying the product's
missing-key message shows that such a lane has none; with state present that
refusal is a failure. Any other outcome exits 1, so the tier reports
cleanup-failed and keeps the tree.
"""
from __future__ import annotations

import argparse
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
from product_e2e_json_snapshot import JsonSnapshot  # noqa: E402

SCHEMA = "bridgevm.windows-hvf-3d-off-product-e2e-request.v2"
FIELDS = frozenset("""schema_version job_id commit campaign_mode lane nonce vm_name vm_slug three_d_injection
    app_bundle_path app_executable_path runner_path firmware_path secure_boot_policy_path iso_path
    bundled_vars_seed_path guest_payload_path guest_payload_manifest_path work_parent work_parent_identity
    work_identity lane_root library_root_path share_path disk_path vars_path vtpm_state_path snapshot_path
    secure_boot_receipt_path guest_evidence_path""".split())
BUNDLE = {"disk_path": "disks/hvf-target.raw", "vars_path": "metadata/hvf-vars.fd", "vtpm_state_path": "metadata/vtpm",
          "snapshot_path": "metadata/snapshots/latest.snapshot",
          "secure_boot_receipt_path": "metadata/secure-boot-provisioning.json",
          "guest_evidence_path": "metadata/product-e2e-guest-evidence.json"}
# VTPMStateSecurityError.missingKeyForExistingState as VTPMLifecycleCommand prints it.
MISSING_KEY = ("ERROR: 기존 vTPM 상태의 Keychain 키가 없습니다. 상태를 덮어쓰지 않았습니다. "
               "복구 키를 복원하거나 명시적으로 TPM을 재설정하세요.")
FORGOT = "forgot_imported_key="
PATTERNS = {"job_id": re.compile(WORK.JOB + r"\Z"), "commit": re.compile(r"[0-9a-f]{40}\Z"),
            "nonce": re.compile(r"[0-9a-f]{64}\Z")}
LANE = re.compile(r"lane-([1-3])\Z")
WORK_NAME = re.compile(r"bridgevm-e2e-" + WORK.JOB + r"\.[A-Za-z0-9]{6}\Z")
TIMEOUT_SECONDS = 120
OUTPUT_LIMIT = 65_536


class Refusal(ValueError):
    pass


def bounded_json(path: Path, what: str) -> object:
    try:
        return JsonSnapshot.read(path).value
    except (OSError, ValueError) as error:
        raise Refusal(f"{what} is not one bounded regular JSON file") from error


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
        raise Refusal("app executable differs from its sealed SHA-256; nothing was run")


def matches(request: dict, field: str) -> bool:
    return type(request[field]) is str and PATTERNS[field].match(request[field]) is not None


def lane_request(work: Path, lane: Path, ordinal: int, mode: str, bundle: Path, executable: Path) -> dict | None:
    """Apply T17Contract's identity and fixed-path rules to the lane's own request, or refuse."""
    path = lane / "request.json"
    if not os.path.lexists(path):
        if os.listdir(lane):
            raise Refusal(f"lane {ordinal} holds files but no request")
        return None
    request = bounded_json(path, f"lane {ordinal} request")
    if not isinstance(request, dict) or set(request) != FIELDS:
        raise Refusal(f"lane {ordinal} request does not have the exact T17 fields")
    prefix = request["nonce"][:12] if matches(request, "nonce") else None
    if not (prefix and request["schema_version"] == SCHEMA and matches(request, "job_id") and matches(request, "commit")
            and request["campaign_mode"] == mode and type(request["lane"]) is int and request["lane"] == ordinal
            and request["three_d_injection"] is False and request["vm_name"] == f"BridgeVM T17 Lane {ordinal} {prefix}"
            and request["vm_slug"] == f"bridgevm-t17-lane-{ordinal}-{prefix}"):
        raise Refusal(f"lane {ordinal} request identity is not its T17 lane slug")
    library = lane / "library"
    fixed = {"lane_root": lane, "library_root_path": library, "share_path": lane / "share",
             "app_bundle_path": bundle, "app_executable_path": executable,
             **{field: library / request["vm_slug"] / "bundle.vmbridge" / name for field, name in BUNDLE.items()}}
    if any(request[field] != str(value) for field, value in fixed.items()) or WORK.validate(request, "e2e") != work:
        raise Refusal(f"lane {ordinal} request names a path outside its own lane")
    return request


def only_registration(library: Path, slug: str, ordinal: int) -> None:
    """Refuse a lane library that holds a VM whose key this lane cannot account for."""
    for name in os.listdir(library):
        mode = os.lstat(library / name).st_mode
        source_cache = name == "Derived" and stat.S_ISDIR(mode) and not os.path.lexists(library / name / "vm.json")
        if name != slug and not stat.S_ISREG(mode) and not source_cache:
            raise Refusal(f"lane {ordinal} library holds a VM registration other than its slug")


def lane_state(request: dict, ordinal: int) -> tuple[Path | None, bool]:
    """Return the lane's vTPM state directory, or None when absent, and whether it holds data."""
    library, slug = Path(request["library_root_path"]), request["vm_slug"]
    chain = [library, library / slug, library / slug / "bundle.vmbridge", library / slug / "bundle.vmbridge/metadata",
             Path(request["vtpm_state_path"])]
    for index, path in enumerate(chain):
        try:
            mode = os.lstat(path).st_mode
        except FileNotFoundError:
            return None, False
        if not stat.S_ISDIR(mode):
            raise Refusal(f"lane {ordinal} vTPM state path has a link or non-directory component")
        if index == 0:
            only_registration(library, slug, ordinal)
    return chain[-1], bool(os.listdir(chain[-1]))


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


def forget(executable: Path, digest: str, request: dict, ordinal: int, state: Path | None, holds_data: bool) -> None:
    folder = Path(tempfile.mkdtemp(prefix="vtpm-key-forget.", dir=request["lane_root"]))
    if state is None:
        state = folder / "empty-state"
        state.mkdir(mode=0o700)
    slug, package, code = request["vm_slug"], folder / "vtpm-recovery.json", folder / "vtpm-recovery-code.txt"
    identity = ["--stable-vm-id", slug, "--state-dir", str(state), "--package", str(package), "--recovery-code-file", str(code)]
    exported = run(executable, digest, ["export", *identity])
    if exported.returncode != 0:
        missing = exported.returncode == 1 and MISSING_KEY in exported.stderr.decode("utf-8", "replace").splitlines()
        if holds_data or not missing or os.path.lexists(package) or os.path.lexists(code):
            raise Refusal(f"lane {ordinal} key was not exported" + (" although its vTPM state holds data" if holds_data
                          else " and the product did not report it missing"))
        return
    forgotten = run(executable, digest, ["forget-import", *identity])
    lines = [line for line in forgotten.stdout.decode("utf-8", "replace").splitlines() if line.startswith(FORGOT)]
    if forgotten.returncode != 0 or lines != [FORGOT + slug]:
        raise Refusal(f"lane {ordinal} key was not forgotten: forget-import gave no exact proof for its slug")


def forget_lanes(work: Path, verified: Path) -> None:
    mode, bundle, executable, digest = sealed_app(verified)
    require_sealed(executable, digest)
    WORK.directory(work)
    if not WORK_NAME.match(work.name):
        raise Refusal("work tree is not a T17 job tree")
    lanes = []
    for name in sorted(os.listdir(work)):
        match = LANE.match(name)
        if not match or int(match.group(1)) > (3 if mode == "release" else 1) or not stat.S_ISDIR(os.lstat(work / name).st_mode):
            raise Refusal(f"work tree holds an entry that is not a {mode} lane: {name!r}")
        ordinal = int(match.group(1))
        request = lane_request(work, work / name, ordinal, mode, bundle, executable)
        if request is not None:
            lanes.append((request, ordinal, *lane_state(request, ordinal)))
    for request, ordinal, state, holds_data in lanes:
        forget(executable, digest, request, ordinal, state, holds_data)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Forget each T17 lane's vTPM key before its tree is removed.")
    parser.add_argument("--work", type=Path, required=True)
    parser.add_argument("--verified", type=Path, required=True)
    args = parser.parse_args(argv)
    try:
        forget_lanes(args.work, args.verified)
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        print(f"T17 lane vTPM key forget refused: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
