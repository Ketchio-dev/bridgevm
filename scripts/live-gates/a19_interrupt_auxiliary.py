#!/usr/bin/env python3
"""Two helper-only interruption cases over isolated clobbered media clones."""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys

from a19_interrupt_auxiliary_io import Commands, DIGEST_FIELDS, MANIFEST_FIELDS, manifest, pair, parsed, read_record, stop_proof
from a19_interrupt_restore_child import private_new
from a19_interrupt_stop_points import CREATE_EXPORT, CREATE_VM_ID, SWAP_RESTORE, helper_command, regular, stable_root, staging

VM_ID = "a19-native-cli-live"


def execute(helper: Path, arguments: list[str], commands: Commands, output: Path, label: str) -> dict:
    return parsed(commands.run([str(helper), *arguments], output / label), MANIFEST_FIELDS)


def interrupt(helper: Path, snapshot: Path, disk: Path, variables: Path,
              output: Path, point, commands: Commands, deadline: int) -> None:
    observer = Path(__file__).with_name("a19_interrupt_restore_child.py")
    commands.run([sys.executable, str(observer), str(helper), str(snapshot), str(disk),
                  str(variables), str(output), "--stop-point", point.name,
                  "--deadline", str(deadline)], output / "observer.stdout", deadline + 15)
    stop_proof(output, point, staging(point, stable_root(disk, variables), snapshot) / "disk.raw")


def swap_case(helper: Path, snapshot: Path, disk: Path, variables: Path,
              output: Path, commands: Commands, deadline: int, original: dict) -> None:
    seed = disk.parent / "seed.snapshot"
    quota = sum(path.stat().st_size for path in (disk, variables))
    seeded = execute(helper, ["create", str(disk), str(variables), str(seed), CREATE_VM_ID,
                              str(quota)], commands, output, "seed-create.stdout")
    if seeded != manifest(seed):
        raise ValueError("auxiliary seed creation result differs from media")
    if execute(helper, ["restore", str(seed), str(disk), str(variables)], commands, output, "seed-restore.stdout") != seeded:
        raise ValueError("auxiliary seed restore result differs from verified seed")
    before = commands.digest(helper, disk, variables, output, "preinterrupt")
    if before != {field: seeded[field] for field in DIGEST_FIELDS} or before["disk_sha256"] == original["disk_sha256"]:
        raise ValueError("swap auxiliary generation must contain distinct authenticated clobber bytes")
    interrupt(helper, snapshot, disk, variables, output, SWAP_RESTORE, commands, deadline)
    if commands.digest(helper, disk, variables, output, "postkill") != before:
        raise ValueError("swap interruption changed selected media")
    retried = execute(helper, ["restore", str(snapshot), str(disk), str(variables)], commands, output, "retry.stdout")
    if retried != original or commands.digest(helper, disk, variables, output, "postretry") != {
            field: original[field] for field in DIGEST_FIELDS}:
        raise ValueError("swap retry did not restore original snapshot")


def create_case(helper: Path, disk: Path, variables: Path, output: Path,
                commands: Commands, deadline: int) -> None:
    destination = disk.parent / "export.snapshot"
    before = commands.digest(helper, disk, variables, output, "source")
    preinterrupt_exists = os.path.lexists(destination)
    if preinterrupt_exists:
        raise ValueError("create auxiliary destination must start absent")
    interrupt(helper, destination, disk, variables, output, CREATE_EXPORT, commands, deadline)
    postkill_exists = os.path.lexists(destination)
    if postkill_exists:
        raise ValueError("create interruption published its fresh destination")
    with private_new(output / "destination-state.private.json", "w") as state:
        json.dump({"destination_path": str(destination), "preinterrupt_exists": preinterrupt_exists,
                   "postkill_exists": postkill_exists}, state)
        state.write("\n")
    if commands.digest(helper, disk, variables, output, "source-postkill") != before:
        raise ValueError("create interruption changed its source")
    argv = helper_command(CREATE_EXPORT, helper, destination, disk, variables, stable_root(disk, variables))
    retried = execute(helper, argv[1:], commands, output, "retry.stdout")
    authenticated = manifest(destination)
    if (retried != authenticated or authenticated["vm_id"] != CREATE_VM_ID or
            {field: authenticated[field] for field in DIGEST_FIELDS} != before):
        raise ValueError("create retry manifest differs from source")
    if commands.digest(helper, destination / "disk.raw", destination / "vars.fd", output, "postretry") != before:
        raise ValueError("create retry pair differs from source")
    if commands.digest(helper, disk, variables, output, "source-postretry") != before:
        raise ValueError("create retry changed its source")
    with private_new(output / "retry-manifest.json", "wb") as retained:
        retained.write(read_record(destination / "manifest.json"))


def run(helper: Path, snapshot: Path, disk: Path, variables: Path, output: Path,
        deadline: int = 300, budget: int = 1800, clone=None) -> None:
    if not 1 <= deadline <= 300 or not 1 <= budget <= 1800:
        raise ValueError("auxiliary deadlines exceed declared limits")
    if any(not regular(path) for path in (helper, disk, variables)):
        raise ValueError("auxiliary inputs must be regular files")
    helper, snapshot, disk, variables, output = (path.resolve(strict=True) for path in
                                                (helper, snapshot, disk, variables, output))
    if not helper.is_absolute() or not os.access(helper, os.X_OK):
        raise ValueError("auxiliary helper must be executable")
    original = manifest(snapshot)
    if original["vm_id"] != VM_ID:
        raise ValueError("auxiliary snapshot is not the declared native VM")
    original_manifest = read_record(snapshot / "manifest.json")
    source = pair(disk, variables)
    if source["disk_sha256"] == original["disk_sha256"]:
        raise ValueError("auxiliary source is not the distinct phase3 clobbered disk")
    live = output / "live"
    if live.is_symlink() or not live.is_dir():
        raise ValueError("auxiliary live directory must be a real owned directory")
    work = live / "auxiliary"
    work.mkdir(mode=0o700)
    commands = Commands(budget)
    try:
        for name, operation in (("swap", swap_case), ("create", create_case)):
            owned, retained = work / name, output / f"aux-{name}"
            owned.mkdir(mode=0o700)
            retained.mkdir(mode=0o700)
            own_disk, own_vars = owned / "disk.raw", owned / "vars.fd"
            for member, origin in ((own_disk, disk), (own_vars, variables)):
                if clone is None:
                    commands.clone(origin, member, retained / f"clone-{member.name}.stdout")
                else:
                    clone(origin, member)
            if pair(own_disk, own_vars) != source:
                raise ValueError("auxiliary clones differ from their sources")
            if name == "swap":
                operation(helper, snapshot, own_disk, own_vars, retained, commands, deadline, original)
            else:
                operation(helper, own_disk, own_vars, retained, commands, deadline)
            if pair(own_disk, own_vars) != source:
                raise ValueError("auxiliary case changed logical clone source bytes")
        if manifest(snapshot) != original or read_record(snapshot / "manifest.json") != original_manifest or pair(disk, variables) != source:
            raise ValueError("auxiliary cases changed original snapshot or phase3 sources")
    finally:
        if commands.cleanup_verified:
            shutil.rmtree(work)
        else:
            with private_new(work / "cleanup-unproven", "w") as flag:
                flag.write("owned auxiliary helper group remains unproven\n")


def interrupted(_signal, _frame):
    raise InterruptedError("auxiliary orchestration interrupted")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    for field in ("helper", "snapshot", "disk", "vars", "output"):
        parser.add_argument(field, type=Path)
    args = parser.parse_args()
    for sig in (signal.SIGINT, signal.SIGTERM):
        signal.signal(sig, interrupted)
    try:
        run(args.helper, args.snapshot, args.disk, args.vars, args.output)
    except (OSError, UnicodeError, ValueError, RuntimeError, TimeoutError, subprocess.SubprocessError) as error:
        print(f"FAIL: A19 auxiliary orchestration: {type(error).__name__}: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
