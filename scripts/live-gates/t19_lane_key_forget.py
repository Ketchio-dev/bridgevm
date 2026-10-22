#!/usr/bin/env python3
"""Account for every T19 lane's imported vTPM key through the sealed app before cleanup removes the tree.

An import installs the source's vTPM key under the new VM's slug before the product publishes vm.json. The helper
forgets it at the end of its lane (A9ImportedKeyCleanup), but not when it crashes, hangs, is killed or its cleanup
fails, and never when the tier stops before or between lanes. This repeats that accounting from the tier: the
sealed app's own `--vtpm-lifecycle forget-import` for the requested slug and every real directory in the lane
library except an unregistered `Derived` source cache. Each call gets the lane's read-only inputs, re-hashed
against their seal: the vTPM state the recovery package was made from (never the lane's booted state, which no
longer matches the package), the package and its code. The product deletes only a stored key equal to the packaged
one. A call counts only as exit 0 with exactly one proof line, for its slug, or as exit 1 with the product's
missing-key line. Every candidate of every lane is attempted and every refusal reported, but no call follows a
timeout or a changed executable. Any refusal exits 1, so the tier reports cleanup-failed and keeps the tree.
Refusals name no private path.
"""
from __future__ import annotations

import argparse
import errno
import hashlib
import importlib.util
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
import unicodedata

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import product_e2e_work as WORK  # noqa: E402
from product_e2e_identity import is_sha256  # noqa: E402
from product_e2e_json_snapshot import JsonSnapshot  # noqa: E402

_SPEC = importlib.util.spec_from_file_location("t19_import_manifest", HERE / "windows-import-product-e2e-manifest.py")
MANIFEST = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(MANIFEST)

SCHEMA = "bridgevm.windows-hvf-import-product-e2e-request.v1"
FIELDS = frozenset("""schema_version job_id commit campaign_mode work_parent work_parent_identity work_identity lane
    nonce vm_name vm_slug three_d_injection app_bundle_path app_executable_path runner_path source_disk_path
    source_vars_path source_vtpm_path source_vtpm_package_path source_vtpm_code_path lane_root library_root_path
    share_path disk_path vars_path vtpm_state_path snapshot_path guest_evidence_path""".split())
APP = {"app_bundle_path": "app_bundle", "app_executable_path": "app_executable", "runner_path": "runner"}
INPUTS = {"source_disk_path": "windows.raw", "source_vars_path": "vars.fd", "source_vtpm_path": "vtpm",
          "source_vtpm_package_path": "vtpm-recovery.json", "source_vtpm_code_path": "vtpm-recovery-code.txt"}
BUNDLE = {"disk_path": "disks/hvf-target.raw", "vars_path": "metadata/hvf-vars.fd", "vtpm_state_path": "metadata/vtpm",
          "snapshot_path": "metadata/snapshots/latest.snapshot",
          "guest_evidence_path": "metadata/product-e2e-guest-evidence.json"}
SEALED = {"source_vtpm": "vtpm", "source_vtpm_package": "vtpm-recovery.json", "source_vtpm_code": "vtpm-recovery-code.txt"}
# VTPMStateSecurityError.missingKeyForExistingState as VTPMImportCleanupCommand prints it.
MISSING_KEY = ("ERROR: 기존 vTPM 상태의 Keychain 키가 없습니다. 상태를 덮어쓰지 않았습니다. "
               "복구 키를 복원하거나 명시적으로 TPM을 재설정하세요.")
FORGOT = "forgot_imported_key="
PATTERNS = {"job_id": re.compile(WORK.JOB + r"\Z"), "commit": re.compile(r"[0-9a-f]{40}\Z"),
            "nonce": re.compile(r"[0-9a-f]{64}\Z")}
LANE = re.compile(r"lane-([1-3])\Z")
WORK_NAME = re.compile(r"bridgevm-import-e2e-" + WORK.JOB + r"\.[A-Za-z0-9]{6}\Z")
LIBRARY_LIMIT = 16  # A9LaneRegistrations.limit: more entries than one lane creates
TIMEOUT_SECONDS = 120
OUTPUT_LIMIT = 65_536


class Refusal(ValueError):
    pass


class Stop(Refusal):
    """A refusal after which no further app call is made."""


def described(error: BaseException) -> str:
    """An error's class, and an OSError's errno name, so that no private path is printed."""
    if isinstance(error, OSError):
        return f"{type(error).__name__} {errno.errorcode.get(error.errno or 0, 'without errno')}"
    return type(error).__name__


def bounded_json(path: Path, what: str) -> object:
    try:
        return JsonSnapshot.read(path).value
    except (OSError, ValueError) as error:
        raise Refusal(f"{what} is not one bounded regular JSON file") from error


def sealed_inputs(verified: Path) -> tuple[str, Path, str, dict, dict]:
    """The campaign mode, app executable and its SHA-256, the sealed app paths and the lane vTPM input seals."""
    value = bounded_json(verified, "verified inputs")
    try:
        assets = value["assets"]
        mode, apps = value["campaign_mode"], {field: assets[key]["path"] for field, key in APP.items()}
        digest, seals = assets["app_executable"]["sha256"], {key: assets[key]["sha256"] for key in SEALED}
    except (KeyError, TypeError) as error:
        raise Refusal("verified inputs lack the sealed app executable or lane vTPM inputs") from error
    paths = list(apps.values())
    if (value.get("verified") is not True or mode not in ("pilot", "release")
            or not all(type(path) is str and path.startswith("/") and os.path.normpath(path) == path for path in paths)
            or apps["app_executable_path"] != apps["app_bundle_path"] + "/Contents/MacOS/BridgeVMControl"
            or not is_sha256(digest) or not all(is_sha256(seal) for seal in seals.values())):
        raise Refusal("verified inputs do not seal a packaged app executable and lane vTPM inputs")
    return mode, Path(apps["app_executable_path"]), digest, apps, seals


def require_sealed(executable: Path, digest: str) -> None:
    """Stop unless the app executable is still the regular file with its sealed SHA-256."""
    try:
        descriptor = os.open(executable, os.O_RDONLY | os.O_NOFOLLOW | os.O_CLOEXEC)
        try:
            if not stat.S_ISREG(os.fstat(descriptor).st_mode):
                raise Stop("sealed app executable is not a regular file")
            hasher = hashlib.sha256()
            for chunk in iter(lambda: os.read(descriptor, 1 << 20), b""):
                hasher.update(chunk)
        finally:
            os.close(descriptor)
    except OSError as error:
        raise Stop(f"sealed app executable cannot be read ({described(error)})") from error
    if hasher.hexdigest() != digest:
        raise Stop("app executable differs from its sealed SHA-256")


def matches(request: dict, field: str) -> bool:
    return type(request[field]) is str and PATTERNS[field].match(request[field]) is not None


def lane_request(work: Path, lane: Path, ordinal: int, mode: str, apps: dict) -> dict | None:
    """The lane's request once every field is what make-windows-import-product-e2e-request.py writes; None if absent."""
    path = lane / "request.json"
    if not os.path.lexists(path):
        if os.listdir(lane):
            raise Refusal(f"lane {ordinal} holds files but no request")
        return None
    request = bounded_json(path, f"lane {ordinal} request")
    if not isinstance(request, dict) or set(request) != FIELDS:
        raise Refusal(f"lane {ordinal} request does not have the exact T19 fields")
    prefix = request["nonce"][:12] if matches(request, "nonce") else None
    if not (prefix and request["schema_version"] == SCHEMA and matches(request, "job_id") and matches(request, "commit")
            and request["campaign_mode"] == mode and type(request["lane"]) is int and request["lane"] == ordinal
            and request["three_d_injection"] is False and request["vm_name"] == f"BridgeVM A9 Import Lane {ordinal} {prefix}"
            and request["vm_slug"] == f"bridgevm-a9-import-lane-{ordinal}-{prefix}"):
        raise Refusal(f"lane {ordinal} request identity is not its T19 lane slug")
    bundle = lane / "library" / request["vm_slug"] / "bundle"
    fixed = {**apps, "lane_root": lane, "library_root_path": lane / "library", "share_path": lane / "share",
             **{field: lane / "inputs" / name for field, name in INPUTS.items()},
             **{field: bundle / name for field, name in BUNDLE.items()}}
    if any(request[field] != str(value) for field, value in fixed.items()):
        raise Refusal(f"lane {ordinal} request names a path outside its own lane")
    try:
        bound = WORK.validate(request, "import-e2e")
    except ValueError as error:
        raise Refusal(f"lane {ordinal} work binding refused: {error}") from error
    if bound != work:
        raise Refusal(f"lane {ordinal} request is bound to another work tree")
    return request


def lane_inputs(lane: Path, ordinal: int, seals: dict) -> Path:
    """The lane's inputs folder once its vTPM state, package and code are unlinked and match their seal."""
    inputs = lane / "inputs"
    kinds = {inputs: stat.S_ISDIR, inputs / "vtpm": stat.S_ISDIR, inputs / "vtpm-recovery.json": stat.S_ISREG,
             inputs / "vtpm-recovery-code.txt": stat.S_ISREG}
    if not all(kind(os.lstat(path).st_mode) for path, kind in kinds.items()):
        raise Refusal(f"lane {ordinal} vTPM input is a link or has the wrong type")
    try:
        actual = {key: MANIFEST.tree_hash(inputs / name, allow_symlinks=False) if key == "source_vtpm"
                  else MANIFEST.file_hash(inputs / name) for key, name in SEALED.items()}
    except ValueError as error:
        raise Refusal(f"lane {ordinal} vTPM state holds a link or special file") from error
    if actual != seals:
        raise Refusal(f"lane {ordinal} vTPM state, package or code differs from its seal")
    return inputs


def safe(name: str) -> bool:
    """A9LaneRegistrations.safe, which also excludes Unicode format characters; an undecodable name is unsafe."""
    try:
        size = len(name.encode("utf-8"))
    except UnicodeEncodeError:
        return False
    return (name not in ("", ".", "..") and size <= 255 and "/" not in name
            and not any(unicodedata.category(character) in ("Cc", "Cf") for character in name))


def candidates(request: dict, lane: Path, ordinal: int, refusals: list[str]) -> list[str]:
    """A9KeyCleanupCandidates: the requested slug and every real library directory but an unregistered Derived cache."""
    slug, library = request["vm_slug"], lane / "library"
    try:
        if not stat.S_ISDIR(os.lstat(library).st_mode):
            refusals.append(f"lane {ordinal} library is not a real directory")
            return [slug]
        entries = os.listdir(library)
    except FileNotFoundError:
        return [slug]
    except OSError as error:
        refusals.append(f"lane {ordinal} library cannot be listed ({described(error)})")
        return [slug]
    if len(entries) > LIBRARY_LIMIT:
        refusals.append(f"lane {ordinal} library holds more entries than one lane creates")
        return [slug]
    names = {slug}
    for name in entries:
        try:
            mode = os.lstat(library / name).st_mode
            cache = name == "Derived" and stat.S_ISDIR(mode) and not os.path.lexists(library / name / "vm.json")
        except OSError as error:
            refusals.append(f"lane {ordinal} library entry cannot be read ({described(error)})")
            continue
        if stat.S_ISREG(mode) or cache:
            continue
        if stat.S_ISDIR(mode) and safe(name):
            names.add(name)
        else:
            refusals.append(f"lane {ordinal} library holds a link, special file or unsafe name")
    return sorted(names)


def forget(executable: Path, digest: str, slug: str, inputs: Path) -> str:
    """One forget-import judged by the product's own words: 'forgotten' or 'absent', otherwise a refusal."""
    require_sealed(executable, digest)
    command = [str(executable), "--vtpm-lifecycle", "forget-import", "--stable-vm-id", slug,
               "--state-dir", str(inputs / "vtpm"), "--package", str(inputs / "vtpm-recovery.json"),
               "--recovery-code-file", str(inputs / "vtpm-recovery-code.txt")]
    try:
        completed = subprocess.run(command, stdin=subprocess.DEVNULL, capture_output=True,
                                   timeout=TIMEOUT_SECONDS, check=False)
    except subprocess.TimeoutExpired as error:
        raise Stop(f"forget-import did not finish within {TIMEOUT_SECONDS} s") from error
    except OSError as error:
        raise Refusal(f"forget-import could not start ({described(error)})") from error
    if len(completed.stdout) > OUTPUT_LIMIT or len(completed.stderr) > OUTPUT_LIMIT:
        raise Refusal(f"forget-import output exceeds {OUTPUT_LIMIT} bytes")
    proofs = [line for line in completed.stdout.decode("utf-8", "replace").splitlines() if line.startswith(FORGOT)]
    if completed.returncode == 0 and proofs == [FORGOT + slug]:
        return "forgotten"
    if (completed.returncode == 1 and not proofs
            and MISSING_KEY in completed.stderr.decode("utf-8", "replace").splitlines()):
        return "absent"
    raise Refusal(f"forget-import exited {completed.returncode} without the product's proof or missing-key line")


def account(work: Path, verified: Path) -> tuple[list[str], dict[str, int]]:
    """Attempt every candidate of every lane; return the refusals and the product's outcome counts."""
    mode, executable, digest, apps, seals = sealed_inputs(verified)
    require_sealed(executable, digest)
    try:
        WORK.directory(work)
    except ValueError as error:
        raise Refusal(f"work tree refused: {error}") from error
    if not WORK_NAME.match(work.name):
        raise Refusal("work tree is not a T19 job tree")
    refusals: list[str] = []
    calls: list[tuple[str, str, Path]] = []
    ordinals: list[int] = []
    for name in sorted(os.listdir(work)):
        match = LANE.match(name)
        ordinal = int(match.group(1)) if match else 0
        try:
            if not ordinal or ordinal > (3 if mode == "release" else 1) or not stat.S_ISDIR(os.lstat(work / name).st_mode):
                refusals.append(f"work tree holds an entry that is not a {mode} lane")
                continue
            ordinals.append(ordinal)
            request = lane_request(work, work / name, ordinal, mode, apps)
            if request is not None:
                inputs = lane_inputs(work / name, ordinal, seals)
                for slug in candidates(request, work / name, ordinal, refusals):
                    kind = "requested slug" if slug == request["vm_slug"] else "library directory"
                    calls.append((f"lane {ordinal} {kind}", slug, inputs))
        except Refusal as refusal:
            refusals.append(str(refusal))
        except OSError as error:
            refusals.append(f"{f'lane {ordinal}' if ordinal else 'work tree entry'} cannot be read ({described(error)})")
    if ordinals != list(range(1, len(ordinals) + 1)):
        refusals.append("work tree lanes do not run consecutively from lane 1")
    counts = {"forgotten": 0, "absent": 0}
    for index, (what, slug, inputs) in enumerate(calls):
        try:
            counts[forget(executable, digest, slug, inputs)] += 1
        except Stop as stop:
            skipped = len(calls) - index - 1
            refusals.append(f"{what}: {stop}" + (f"; {skipped} further forget-import call(s) not made" if skipped else ""))
            break
        except Refusal as refusal:
            refusals.append(f"{what}: {refusal}")
    return refusals, counts


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Account for each T19 lane's imported vTPM key before its tree is removed.")
    parser.add_argument("--work", type=Path, required=True)
    parser.add_argument("--verified", type=Path, required=True)
    args = parser.parse_args(argv)
    counts = None
    try:
        refusals, counts = account(args.work, args.verified)
    except Refusal as refusal:
        refusals = [f"{refusal}; nothing was run"]
    except (OSError, ValueError, RuntimeError) as error:
        refusals = [f"{described(error)}; nothing was run"]
    for refusal in refusals:
        print(f"T19 lane vTPM key forget refused: {refusal}", file=sys.stderr)
    if counts is not None:
        print(f"T19 lane vTPM keys: {counts['forgotten']} forgotten, {counts['absent']} absent, {len(refusals)} refusal(s)")
    return 1 if refusals else 0


if __name__ == "__main__":
    raise SystemExit(main())
