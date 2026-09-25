#!/usr/bin/env python3
"""Strict offline B8 install-cell shape; not connected to a live publisher."""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import tarfile

from b8_clean_install_inputs import (COMMIT, SHA, _canonical, _parse_json, load_manifest,
                                     read_regular, verify_release)

TIER = "t23-b8-clean-install"
SCHEMA = "bridgevm.b8-clean-install-receipt.v1"
JOB = re.compile(r"[A-Za-z0-9][A-Za-z0-9._-]{0,127}\Z")
TIME = re.compile(r"\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ\Z")
HASHES = ("input_manifest_sha256", "release_contract_sha256", "sha256s_sha256",
          "tarball_sha256", "clean_host_attestation_sha256", "installer_bootstrap_sha256",
          "installer_source_sha256", "expected_app_tree_sha256", "expected_executable_sha256",
          "installed_app_tree_sha256", "installed_executable_sha256")
FLAGS = ("clean_machine", "app_absent_before", "app_present_after", "codesign_verified",
         "worker_cleanup_verified", "cell_pass", "criterion_pass", "capability_promotion")
COUNTS = ("sample_count", "guest_boot_count", "installer_exit_code")
PUBLIC = frozenset({"schema", "tier", "criterion", "cell", "host_generation", "host_model",
                    "macos_build", "job_id", "commit", "release_commit", "release_tag",
                    "signing_class", "bundle_id", "outcome", "started_at", "finished_at",
                    *HASHES, *FLAGS, *COUNTS})
PRIVATE = PUBLIC | {"private_attestation_path", "private_artifacts"}


def read_json(path: Path) -> dict:
    return _parse_json(read_regular(path, 65_536))


def _env(path: Path, readonly: bool = False) -> dict[str, str]:
    raw = read_regular(path, 8192)
    if readonly and os.lstat(path).st_mode & 0o222:
        raise ValueError("B8 ledger is writable")
    fields: dict[str, str] = {}
    for line in raw.decode("ascii").splitlines():
        key, separator, item = line.partition("=")
        if not separator or key in fields or not re.fullmatch(r"[a-z0-9_]+", key):
            raise ValueError("B8 job seal has a malformed field")
        fields[key] = item
    return fields


def job_fields(directory: Path) -> dict[str, str]:
    _canonical(directory)
    if not JOB.fullmatch(directory.name) or directory.parent.name != "queued":
        raise ValueError("B8 job directory is unsafe")
    ledger_dir = directory.parent.parent / "job-ledger" / directory.name
    if ledger_dir.is_symlink() or not ledger_dir.is_dir():
        raise ValueError("B8 ledger directory is unsafe")
    job = _env(directory / "job.env")
    ledger = _env(ledger_dir / "entry.env", readonly=True)
    sealed = {"job_id", "tier", "commit", "input_manifest_sha256"}
    if (not sealed <= set(job) or set(ledger) != sealed
            or any(job[key] != ledger[key] for key in sealed)
            or job["job_id"] != directory.name or job["tier"] != TIER
            or not COMMIT.fullmatch(job["commit"])
            or not SHA.fullmatch(job["input_manifest_sha256"])):
        raise ValueError("B8 immutable queue identity differs")
    return job


def _time(value: object) -> datetime:
    if type(value) is not str or not TIME.fullmatch(value):
        raise ValueError("B8 receipt time differs")
    try:
        return datetime.strptime(value, "%Y-%m-%dT%H:%M:%SZ")
    except ValueError as error:
        raise ValueError("B8 receipt time is invalid") from error


def _plain(value: dict, job: dict, manifest: dict, expected: dict, private: bool) -> None:
    if set(value) != (PRIVATE if private else PUBLIC):
        raise ValueError("B8 receipt field set differs")
    fixed = {"schema": SCHEMA, "tier": TIER, "criterion": "B8", "cell": "install",
             "host_generation": "M5", "job_id": job["job_id"], "commit": job["commit"],
             "release_commit": job["commit"], "release_tag": manifest["release_tag"],
             "bundle_id": "dev.bridgevm.control",
             "input_manifest_sha256": job["input_manifest_sha256"],
             "release_contract_sha256": manifest["release_contract_sha256"],
             "sha256s_sha256": manifest["sha256s_sha256"],
             "tarball_sha256": manifest["tarball_sha256"],
             "clean_host_attestation_sha256": manifest["clean_host_attestation_sha256"],
             "expected_app_tree_sha256": expected["app_tree_sha256"],
             "expected_executable_sha256": expected["app_executable_sha256"]}
    if any(value[key] != item or type(value[key]) is not type(item) for key, item in fixed.items()):
        raise ValueError("B8 receipt source, cell or release differs")
    for key in HASHES:
        item = value[key]
        if type(item) is not str or (item != "absent" and not SHA.fullmatch(item)):
            raise ValueError("B8 receipt hash differs: " + key)
    for key in FLAGS:
        if type(value[key]) is not bool:
            raise ValueError("B8 receipt boolean differs: " + key)
    for key in COUNTS:
        if type(value[key]) is not int or value[key] < 0 or value[key] >= 2**63:
            raise ValueError("B8 receipt count differs: " + key)
    if (value["criterion_pass"] or value["capability_promotion"]
            or value["guest_boot_count"] != 0 or value["sample_count"] != 0):
        raise ValueError("one B8 install cannot complete the matrix or boot")
    if value["host_model"] not in ("absent", "Mac17,9") or type(value["host_model"]) is not str:
        raise ValueError("B8 host model differs")
    build = value["macos_build"]
    if type(build) is not str or len(build) > 40 or (build != "absent" and not re.fullmatch(r"[A-Za-z0-9.()_-]+", build)):
        raise ValueError("B8 macOS build differs")
    if _time(value["finished_at"]) < _time(value["started_at"]):
        raise ValueError("B8 receipt time order differs")
    if value["signing_class"] not in ("unverified", "ad-hoc", "development-signed", "developer-id"):
        raise ValueError("B8 signing class differs")
    if value["outcome"] not in ("not-run", "failed", "completed"):
        raise ValueError("B8 outcome differs")
    if value["cell_pass"] or value["clean_machine"] or value["outcome"] == "completed":
        raise ValueError("offline B8 contract cannot verify a physical install")
    if not private:
        for item in value.values():
            if isinstance(item, str) and any(char in item for char in ("/", "\\")):
                raise ValueError("B8 public receipt contains path or private text")


def validate_private(value: dict, job: dict, manifest: dict, expected: dict,
                     diagnostic: Path) -> None:
    _plain(value, job, manifest, expected, True)
    if value["private_attestation_path"] != manifest["clean_host_attestation"]:
        raise ValueError("B8 private attestation path differs")
    artifacts = value["private_artifacts"]
    if not isinstance(artifacts, dict) or not set(artifacts) <= {"installer.log", "release.json"}:
        raise ValueError("B8 private artifact set differs")
    for name, seal in artifacts.items():
        if (not isinstance(seal, dict) or set(seal) != {"bytes", "sha256"}
                or type(seal["bytes"]) is not int or not 0 < seal["bytes"] <= 8_000_000
                or type(seal["sha256"]) is not str or not SHA.fullmatch(seal["sha256"])):
            raise ValueError("B8 private artifact seal differs")
        raw = read_regular(diagnostic / name, 8_000_000)
        if len(raw) != seal["bytes"] or hashlib.sha256(raw).hexdigest() != seal["sha256"]:
            raise ValueError("B8 private artifact changed")
def public_view(value: dict, job: dict, manifest: dict, expected: dict,
                diagnostic: Path) -> dict:
    validate_private(value, job, manifest, expected, diagnostic)
    public = {key: value[key] for key in PUBLIC}
    _plain(public, job, manifest, expected, False)
    return public


def verify_public(directory: Path, manifest_path: Path, assets: Path) -> None:
    job = job_fields(directory)
    raw_manifest = read_regular(manifest_path, 8192)
    if hashlib.sha256(raw_manifest).hexdigest() != job["input_manifest_sha256"]:
        raise ValueError("B8 manifest differs from immutable job seal")
    manifest = load_manifest(manifest_path, job["commit"])
    expected = verify_release(manifest, assets)
    private = read_json(directory / "receipt.json")
    public = read_json(directory / "receipt.public.json")
    derived = public_view(private, job, manifest, expected, directory / "diagnostic")
    _plain(public, job, manifest, expected, False)
    if public != derived:
        raise ValueError("B8 public receipt differs from private-backed view")


def missing(job: dict, manifest: dict, expected: dict) -> dict:
    timestamp = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    value = {key: "absent" for key in HASHES}
    value.update({"schema": SCHEMA, "tier": TIER, "criterion": "B8", "cell": "install",
                  "host_generation": "M5", "host_model": "absent", "macos_build": "absent",
                  "job_id": job["job_id"], "commit": job["commit"], "release_commit": job["commit"],
                  "release_tag": manifest["release_tag"], "signing_class": "unverified",
                  "bundle_id": "dev.bridgevm.control", "outcome": "not-run",
                  "started_at": timestamp, "finished_at": timestamp,
                  "input_manifest_sha256": job["input_manifest_sha256"],
                  "release_contract_sha256": manifest["release_contract_sha256"],
                  "sha256s_sha256": manifest["sha256s_sha256"],
                  "tarball_sha256": manifest["tarball_sha256"],
                  "clean_host_attestation_sha256": manifest["clean_host_attestation_sha256"],
                  "expected_app_tree_sha256": expected["app_tree_sha256"],
                  "expected_executable_sha256": expected["app_executable_sha256"],
                  **{key: False for key in FLAGS}, **{key: 0 for key in COUNTS},
                  "private_attestation_path": manifest["clean_host_attestation"],
                  "private_artifacts": {}})
    return value


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("verify-public",))
    parser.add_argument("job_dir", type=Path)
    parser.add_argument("manifest", type=Path)
    parser.add_argument("assets", type=Path)
    args = parser.parse_args()
    try:
        verify_public(args.job_dir, args.manifest, args.assets)
    except (OSError, ValueError, tarfile.TarError) as error:
        print("B8 offline receipt refused: " + str(error))
        return 2
    print("B8 offline contract: PASS (no live cell)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
