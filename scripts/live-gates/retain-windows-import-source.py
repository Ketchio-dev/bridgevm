#!/usr/bin/env python3
"""Retain one authenticated, stopped T17 lane as private T19 source inputs."""
from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import os
import shutil
import stat
import subprocess
import sys
import uuid
from pathlib import Path
HERE = Path(__file__).resolve().parent
def load_module(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load {path.name}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module
T17 = load_module("t17_receipt", HERE / "write-windows-product-e2e-receipt.py")
T19 = load_module("t19_manifest", HERE / "windows-import-product-e2e-manifest.py")
def load_json(path: Path) -> dict:
    value = T17.load_json(path)
    if not isinstance(value, dict):
        raise ValueError(f"{path.name} is not a JSON object")
    return value

def safe_tree(root: Path) -> list[Path]:
    if not root.is_dir() or root.is_symlink():
        raise ValueError("vTPM state is missing or unsafe")
    entries = sorted(root.rglob("*"))
    if not entries or len(entries) > 1_024:
        raise ValueError("vTPM state is empty or oversized")
    for entry in entries:
        mode = entry.lstat().st_mode
        if stat.S_ISLNK(mode) or not (stat.S_ISDIR(mode) or stat.S_ISREG(mode)):
            raise ValueError("vTPM state contains an unsafe entry")
        if stat.S_ISREG(mode) and entry.stat().st_nlink != 1:
            raise ValueError("vTPM state contains a linked file")
    return entries

def make_private_parent(path: Path) -> None:
    path.mkdir(mode=0o700, parents=True, exist_ok=True)
    if path.is_symlink() or path.stat().st_uid != os.geteuid():
        raise ValueError("retained-source parent is unsafe")
    path.chmod(0o700)


def clone_file(source: Path, destination: Path) -> None:
    subprocess.run(["/bin/cp", "-c", str(source), str(destination)], check=True)


def lock_tree(root: Path) -> None:
    for entry in sorted(root.rglob("*"), reverse=True):
        entry.chmod(0o500 if entry.is_dir() else 0o400)
    root.chmod(0o500)


def write_manifest(path: Path, mode: str, assets: dict[str, tuple[Path, str]]) -> None:
    with path.open("x", encoding="utf-8") as output:
        output.write(f"campaign_mode\t{mode}\n")
        for key in T19.ASSETS:
            candidate, digest = assets[key]
            output.write(f"{key}\t{candidate}\t{digest}\n")


def retain(args: argparse.Namespace) -> None:
    request = load_json(args.request)
    verified = load_json(args.verified)
    stamp = load_json(args.stamp)
    result = T17.lane(
        args.result, job_id=request["job_id"], commit=request["commit"],
        mode=request["campaign_mode"], ordinal=request["lane"], stamp=args.stamp,
    )
    if stamp.get("request_sha256") != T17.digest(args.request):
        raise ValueError("authentication stamp does not seal the request")
    if not all(result[stage] for stage in T17.LANE_STAGES) or result["cleanup_verified"] is not True:
        raise ValueError("T17 lane is not a complete authenticated success")
    if verified.get("verified") is not True or verified.get("campaign_mode") != request["campaign_mode"]:
        raise ValueError("T17 input verification is not valid for this lane")
    assets = verified.get("assets")
    app = Path(request["app_bundle_path"])
    executable = Path(request["app_executable_path"])
    helper = Path(assets["product_helper"]["path"])
    runner = Path(request["runner_path"])
    expected_app_paths = {
        "app_bundle": app, "app_executable": executable,
        "product_helper": helper, "runner": runner,
    }
    if any(Path(assets[key]["path"]) != value for key, value in expected_app_paths.items()):
        raise ValueError("T17 request app paths differ from its verified inputs")
    disk = Path(request["disk_path"])
    variables = Path(request["vars_path"])
    state = Path(request["vtpm_state_path"])
    if not disk.is_file() or disk.is_symlink() or T19.file_hash(disk) != result["final_disk_sha256"]:
        raise ValueError("installed disk differs from authenticated T17 output")
    if (not variables.is_file() or variables.is_symlink()
            or variables.stat().st_size != 64 * 1024 * 1024
            or T19.file_hash(variables) != result["final_vars_sha256"]):
        raise ValueError("UEFI variables differ from authenticated T17 output")
    safe_tree(state)
    state_hash = T19.tree_hash(state, allow_symlinks=False)
    destination = args.destination.absolute()
    if os.path.normpath(str(destination)) != str(destination):
        raise ValueError("retained-source destination is not normalized")
    if destination.exists() or destination.is_symlink():
        raise ValueError("retained-source destination already exists")
    make_private_parent(destination.parent)
    staging = destination.parent / f".{destination.name}.stage-{uuid.uuid4().hex}"
    staging.mkdir(mode=0o700)
    try:
        package = staging / "vtpm-recovery.json"
        code = staging / "vtpm-recovery-code.txt"
        subprocess.run([
            str(executable), "--vtpm-lifecycle", "export",
            "--stable-vm-id", request["vm_slug"], "--state-dir", str(state),
            "--package", str(package), "--recovery-code-file", str(code),
        ], check=True, stdout=subprocess.DEVNULL)
        if (not package.is_file() or package.is_symlink() or not code.is_file()
                or code.is_symlink() or code.stat().st_mode & 0o077):
            raise ValueError("packaged vTPM export did not create private regular files")
        retained_disk = staging / "windows.raw"
        retained_vars = staging / "vars.fd"
        retained_state = staging / "vtpm"
        clone_file(disk, retained_disk)
        clone_file(variables, retained_vars)
        retained_state.mkdir(mode=0o700)
        subprocess.run(["/bin/cp", "-cR", f"{state}/.", str(retained_state)], check=True)
        safe_tree(retained_state)
        disk_hash = T19.file_hash(retained_disk); vars_hash = T19.file_hash(retained_vars)
        retained_state_hash = T19.tree_hash(retained_state, allow_symlinks=False)
        if disk_hash != result["final_disk_sha256"] or vars_hash != result["final_vars_sha256"]:
            raise ValueError("retained media differs from authenticated T17 output")
        if retained_state_hash != state_hash or T19.tree_hash(state, allow_symlinks=False) != state_hash:
            raise ValueError("vTPM state changed while it was retained")
        app_assets = {
            key: (path, assets[key]["sha256"])
            for key, path in expected_app_paths.items()
        }
        source_assets = {
            "source_disk": (destination / retained_disk.name, disk_hash),
            "source_vars": (destination / retained_vars.name, vars_hash),
            "source_vtpm": (destination / retained_state.name, retained_state_hash),
            "source_vtpm_package": (destination / package.name, T19.file_hash(package)),
            "source_vtpm_code": (destination / code.name, T19.file_hash(code)),
        }
        manifest = staging / "t19-input-manifest.tsv"
        write_manifest(manifest, request["campaign_mode"], {**app_assets, **source_assets})
        staging.rename(destination)
        manifest = destination / manifest.name
        verified_output = destination / "t19-verified.json"
        check = T19.verify(manifest)
        if check.get("verified") is not True:
            raise ValueError(f"retained T19 inputs failed verification: {check.get('failure_code')}")
        with verified_output.open("x", encoding="utf-8") as output:
            json.dump(check, output, indent=2, sort_keys=True)
            output.write("\n")
        lock_tree(destination)
    except BaseException:
        cleanup = destination if destination.exists() else staging
        if cleanup.exists():
            for entry in cleanup.rglob("*"):
                try:
                    entry.chmod(0o700 if entry.is_dir() else 0o600)
                except OSError:
                    pass
            cleanup.chmod(0o700)
            shutil.rmtree(cleanup, ignore_errors=True)
        raise
    status = {
        "schema_version": "bridgevm.t17-t19-private-handoff.v1",
        "job_id": request["job_id"], "commit": request["commit"],
        "lane": request["lane"], "destination": str(destination),
        "manifest_sha256": hashlib.sha256((destination / "t19-input-manifest.tsv").read_bytes()).hexdigest(),
        "verified": True,
    }
    with args.status.open("x", encoding="utf-8") as output:
        json.dump(status, output, indent=2, sort_keys=True)
        output.write("\n")
    args.status.chmod(0o600)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("request", "result", "stamp", "verified", "destination", "status"):
        parser.add_argument(f"--{name}", type=Path, required=True)
    args = parser.parse_args()
    try:
        retain(args)
    except (OSError, ValueError, KeyError, json.JSONDecodeError, subprocess.CalledProcessError) as error:
        print(f"T17-to-T19 handoff refused: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
