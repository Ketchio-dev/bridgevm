"""Synthetic pair helper commands for product tier fixtures."""
import hashlib
import json
import shutil
from pathlib import Path


def command(arguments):
    operation, *values = arguments
    if operation == "digest" and len(values) == 2:
        for name, raw in zip(("disk", "vars"), values):
            data = Path(raw).read_bytes()
            print(f"{name}_bytes {len(data)}\n{name}_sha256 {hashlib.sha256(data).hexdigest()}")
        return 0
    if operation == "create" and len(values) == 5:
        disk, variables, raw_destination, vm_id, quota = values
        sources = {"disk": Path(disk), "vars": Path(variables)}
        if sum(path.stat().st_size for path in sources.values()) > int(quota):
            return 1
        destination = Path(raw_destination)
        destination.mkdir()
        manifest = {"format_version": 1, "vm_id": vm_id}
        for name, source in sources.items():
            target = destination / ("disk.raw" if name == "disk" else "vars.fd")
            shutil.copyfile(source, target)
            manifest[f"{name}_bytes"] = target.stat().st_size
            manifest[f"{name}_sha256"] = hashlib.sha256(target.read_bytes()).hexdigest()
        (destination / "manifest.json").write_text(json.dumps(manifest) + "\n")
        return 0
    return 2
