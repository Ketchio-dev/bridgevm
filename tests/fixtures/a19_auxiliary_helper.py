#!/usr/bin/env python3
"""Small synthetic helper with observable staged reads; never runs a guest."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import sys
import time

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "scripts/live-gates"))
from a19_interrupt_stop_points import stable_root


def selected(disk, variables):
    current = stable_root(disk, variables) / "current"
    return (current / "disk.raw", current / "vars.fd") if current.exists() else (disk, variables)


def manifest(directory, vm_id):
    result = {"format_version": 1, "vm_id": vm_id}
    for name, member in (("disk", "disk.raw"), ("vars", "vars.fd")):
        data = (directory / member).read_bytes()
        result[name + "_bytes"] = len(data)
        result[name + "_sha256"] = hashlib.sha256(data).hexdigest()
    return result


def show(value):
    for field in ("format_version", "vm_id", "disk_bytes", "disk_sha256", "vars_bytes", "vars_sha256"):
        print(field, value[field])


def write_manifest(path, value):
    with path.open("w") as output:
        json.dump(value, output)
        output.write("\n")
        output.flush()
        os.fsync(output.fileno())


def main():
    config = json.loads(Path(os.environ.get("A19_FIXTURE_CONFIG", str(Path(sys.argv[0]).with_suffix(".json")))).read_text())
    calls_path = Path(config["calls"])
    calls = json.loads(calls_path.read_text()) if calls_path.exists() else []
    args = sys.argv[1:]
    previous = sum(call == args for call in calls)
    calls.append(args)
    calls_path.write_text(json.dumps(calls))
    fault = config.get("fault", "")
    operation = args[0]
    if operation == "digest":
        for name, path in zip(("disk", "vars"), selected(*map(Path, args[1:]))):
            data = path.read_bytes()
            print(name + "_bytes", len(data))
            print(name + "_sha256", "0" * 64 if fault == "digest-lie" else hashlib.sha256(data).hexdigest())
        return 0
    if operation == "restore":
        source, disk, variables = map(Path, args[1:])
        managed = stable_root(disk, variables)
        managed.mkdir(exist_ok=True)
        destination, stage = managed / "current", managed / "staging"
        interrupted = source.name != "seed.snapshot" and previous == 0
        sources = source / "disk.raw", source / "vars.fd"
        value = json.loads((source / "manifest.json").read_text())
    elif operation == "create":
        disk, variables, destination = map(Path, args[1:4])
        stage = destination.parent / ("." + destination.name + ".staging")
        sources = selected(disk, variables)
        if sum(path.stat().st_size for path in sources) > int(args[5]):
            return 4
        interrupted = destination.name == "export.snapshot" and previous == 0
        value = None
    else:
        return 2
    if fault == "retry-failed" and previous > 0:
        return 11
    if stage.exists():
        shutil.rmtree(stage)
    stage.mkdir(mode=0o700)
    for origin, member in zip(sources, ("disk.raw", "vars.fd")):
        with (stage / member).open("wb") as target:
            target.write(b"wrongseed vars" if fault == "seed-wrong-vars" and destination.name == "seed.snapshot" and member == "vars.fd" else origin.read_bytes())
            target.flush()
            os.fsync(target.fileno())
    if operation == "restore":
        write_manifest(stage / "manifest.json", value)
    if interrupted:
        if fault == "create-published" and operation == "create":
            destination.mkdir()
            (destination / "manifest.json").write_text("{}")
        if fault == "create-source-mutated" and operation == "create":
            disk.write_bytes(b"changed source")
        with (stage / "disk.raw").open("rb"):
            (calls_path.parent / "held.pid").write_text(str(os.getpid()))
            time.sleep(30)
    if operation == "restore" and fault == "swap-noop-retry" and previous > 0:
        shutil.rmtree(stage)
        show(value)
        return 0
    if operation == "create":
        value = manifest(stage, args[4])
        write_manifest(stage / "manifest.json", value)
    if destination.exists():
        shutil.rmtree(destination)
    stage.rename(destination)
    show(value)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
