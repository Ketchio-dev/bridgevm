#!/usr/bin/env python3
"""Create one strict lane-local T17 packaged-product request."""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path


ASSETS = (
    "app_bundle", "app_executable", "runner", "firmware", "secure_boot_policy",
    "iso", "bundled_vars_seed", "guest_payload", "guest_payload_manifest",
)


def unique(pairs: list[tuple[str, object]]) -> dict:
    value: dict[str, object] = {}
    for key, item in pairs:
        if key in value:
            raise ValueError(f"duplicate verified-input field: {key}")
        value[key] = item
    return value


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--verified", type=Path, required=True)
    parser.add_argument("--job-id", required=True)
    parser.add_argument("--commit", required=True)
    parser.add_argument("--mode", choices=("pilot", "release"), required=True)
    parser.add_argument("--lane", type=int, required=True)
    parser.add_argument("--nonce", required=True)
    parser.add_argument("--lane-root", type=Path, required=True)
    args = parser.parse_args()
    verified = json.loads(args.verified.read_text(encoding="utf-8"), object_pairs_hook=unique)
    assets = verified.get("assets")
    if verified.get("verified") is not True or not isinstance(assets, dict):
        raise ValueError("T17 inputs were not verified")
    if any(not isinstance(assets.get(key), dict) or not isinstance(assets[key].get("path"), str) for key in ASSETS):
        raise ValueError("T17 verified inputs are incomplete")
    root = args.lane_root
    raw_root = str(root)
    if not raw_root.startswith("/tmp/bridgevm-e2e-") or raw_root != os.path.normpath(raw_root):
        raise ValueError("lane root is outside the fixed /tmp/bridgevm-e2e-* boundary")
    if not root.is_dir() or root.is_symlink() or any(root.iterdir()):
        raise ValueError("lane root must be an existing empty non-symlink directory")
    nonce_prefix = args.nonce[:12]
    vm_name = f"BridgeVM T17 Lane {args.lane} {nonce_prefix}"
    vm_slug = f"bridgevm-t17-lane-{args.lane}-{nonce_prefix}"
    library = root / "library"
    bundle = library / vm_slug / "bundle.vmbridge"
    request = {
        "schema_version": "bridgevm.windows-hvf-3d-off-product-e2e-request.v2",
        "job_id": args.job_id, "commit": args.commit, "campaign_mode": args.mode,
        "lane": args.lane, "nonce": args.nonce, "three_d_injection": False,
        "vm_name": vm_name, "vm_slug": vm_slug,
        **{f"{key}_path": assets[key]["path"] for key in ASSETS},
        "lane_root": raw_root, "library_root_path": str(library),
        "share_path": f"{raw_root}/share",
        "disk_path": str(bundle / "disks/hvf-target.raw"),
        "vars_path": str(bundle / "metadata/hvf-vars.fd"),
        "vtpm_state_path": str(bundle / "metadata/vtpm"),
        "snapshot_path": str(bundle / "metadata/snapshots/latest.snapshot"),
        "secure_boot_receipt_path": str(bundle / "metadata/secure-boot-provisioning.json"),
        "guest_evidence_path": str(bundle / "metadata/product-e2e-guest-evidence.json"),
    }
    with args.out.open("x", encoding="utf-8") as output:
        json.dump(request, output, indent=2, sort_keys=True)
        output.write("\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
