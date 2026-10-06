#!/usr/bin/env python3
"""Check JSON syntax and the flat QEMU deviation registry's referenced entries."""

import json
from pathlib import Path
import sys


def validate(path: Path) -> None:
    document = json.loads(path.read_text())
    if path.name != "qemu-virt-deviations.json":
        return
    if type(document.get("schema_version")) is not int or document["schema_version"] != 1:
        raise ValueError("unsupported deviation schema_version")
    if document.get("contract") != "qemu-virt-compatible":
        raise ValueError("unsupported deviation contract")
    metadata = {key: document[key] for key in ("schema_version", "contract")}
    modules = document.get("deviation_modules", [])
    if not isinstance(modules, list):
        raise ValueError("deviation_modules must be a list")
    documents = [document]
    seen_modules = set()
    for name in modules:
        if (not isinstance(name, str) or Path(name).name != name or "\\" in name
                or not Path(name).match("qemu-virt-deviations-*.json") or name == path.name or name in seen_modules):
            raise ValueError("deviation modules must be unique same-directory JSON filenames")
        module_path = path.parent / name
        if module_path.is_symlink():
            raise ValueError("deviation modules must not be symlinks")
        module = json.loads(module_path.read_text())
        if any(type(module.get(key)) is not type(value) or module.get(key) != value
               for key, value in metadata.items()):
            raise ValueError(f"deviation module metadata differs: {name}")
        if "deviation_modules" in module:
            raise ValueError(f"nested deviation modules are unsupported: {name}")
        seen_modules.add(name)
        documents.append(module)
    seen_ids = set()
    for module in documents:
        entries = module["deviations"]
        if not isinstance(entries, list):
            raise ValueError("deviations must be a list")
        for entry in entries:
            for key in ("id", "area", "qemu_behavior", "bridgevm_behavior", "impact", "evidence"):
                if not isinstance(entry.get(key), str) or not entry[key].strip():
                    raise ValueError(f"missing or invalid deviation field: {key}")
            if type(entry.get("guest_visible")) is not bool:
                raise ValueError("guest_visible must be a boolean")
            if entry["id"] in seen_ids:
                raise ValueError(f"duplicate deviation ID: {entry['id']}")
            seen_ids.add(entry["id"])


def main() -> int:
    if len(sys.argv) < 2:
        print("usage: check-contract-schema-json.py PATH [PATH ...]", file=sys.stderr)
        return 2
    try:
        for name in sys.argv[1:]:
            validate(Path(name))
    except (OSError, ValueError, KeyError, TypeError, AttributeError) as error:
        print(f"FAIL: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
