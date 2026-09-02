#!/usr/bin/env python3
"""Write the machine-readable contract for a general BridgeVM preview."""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
import pathlib
import re
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parent.parent
REGISTRY = ROOT / "capabilities/windows-hvf.json"
VERSION = re.compile(r"^v[0-9]+(?:\.[0-9]+){2}(?:[.-][0-9A-Za-z]+)*$")
COMMIT = re.compile(r"^[0-9a-f]{40}$")


def build_manifest(registry: dict, version: str, commit: str) -> dict:
    if not VERSION.fullmatch(version):
        raise ValueError(f"invalid release version: {version}")
    if not COMMIT.fullmatch(commit):
        raise ValueError("source commit must be a full lowercase Git SHA")
    if registry.get("product_state") != "ENGINEERING_PREVIEW":
        raise ValueError("general-preview policy expects ENGINEERING_PREVIEW")
    criteria = {item.get("id"): item for item in registry.get("criteria", [])}
    a9 = criteria.get("A9", {})
    if a9.get("state") != "OPEN" or a9.get("release_blocking") is not True:
        raise ValueError("A9 must remain release-blocking and OPEN for this policy")
    if "3D-off install/import" not in a9.get("known_defect", ""):
        raise ValueError("A9 must disclose that product install/import is 3D-off")

    registry_bytes = json.dumps(
        registry, ensure_ascii=False, separators=(",", ":"), sort_keys=True
    ).encode()
    return {
        "schema_version": 1,
        "project": "BridgeVM",
        "version": version,
        "source_commit": commit,
        "channel": "general-preview",
        "product_state": "ENGINEERING_PREVIEW",
        "macos": {"developer_id_signed": False, "notarized": False},
        "windows_graphics": {
            "kernel_driver_included": False,
            "test_signing_required": False,
            "product_injection_available": False,
            "install_mode": "3d-off",
        },
        "capability_registry": {
            "path": "capabilities/windows-hvf.json",
            "reviewed": registry.get("reviewed"),
            "tested_commit": registry.get("tested_commit"),
            "canonical_json_sha256": hashlib.sha256(registry_bytes).hexdigest(),
        },
    }


def self_test(registry: dict) -> None:
    manifest = build_manifest(registry, "v1.2.3-preview.1", "a" * 40)
    assert manifest["channel"] == "general-preview"
    assert manifest["windows_graphics"]["kernel_driver_included"] is False
    assert manifest["windows_graphics"]["product_injection_available"] is False
    changed = copy.deepcopy(registry)
    next(item for item in changed["criteria"] if item["id"] == "A9")["state"] = "PROVEN"
    try:
        build_manifest(changed, "v1.2.3", "b" * 40)
    except ValueError:
        pass
    else:
        raise AssertionError("a promoted A9 must require an intentional policy update")
    workflow = (ROOT / ".github/workflows/release.yml").read_text()
    for required in (
        "generate-general-preview-manifest.py",
        "hvf-windows-product-injection-deny-smoke.sh",
        "Refuse Windows kernel packages in the general artifact",
        "-iname '*.sys'",
        "draft: true", "fetch-depth: 0", "git rev-parse --is-shallow-repository",
    ):
        assert required in workflow, f"release workflow lost boundary: {required}"
    print("general preview manifest: PASS")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--version")
    parser.add_argument("--commit")
    parser.add_argument("--output", type=pathlib.Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    registry = json.loads(REGISTRY.read_text())
    if args.self_test:
        self_test(registry)
        return 0
    if not (args.version and args.commit and args.output):
        parser.error("--version, --commit, and --output are required")
    try:
        manifest = build_manifest(registry, args.version, args.commit)
    except ValueError as error:
        print(f"general preview manifest: FAIL ({error})", file=sys.stderr)
        return 1
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(
        "w", encoding="utf-8", dir=args.output.parent, delete=False
    ) as handle:
        json.dump(manifest, handle, indent=2, sort_keys=True)
        handle.write("\n")
        temporary = pathlib.Path(handle.name)
    temporary.replace(args.output)
    print(args.output)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
