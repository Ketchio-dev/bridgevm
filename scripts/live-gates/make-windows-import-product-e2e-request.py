#!/usr/bin/env python3
"""Create one fail-closed lane request from authenticated installed media clones."""
from __future__ import annotations
import argparse, hashlib, importlib.util, json, os, stat
from pathlib import Path
HERE = Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location("import_manifest", HERE / "windows-import-product-e2e-manifest.py")
MANIFEST = importlib.util.module_from_spec(SPEC); SPEC.loader.exec_module(MANIFEST)
APP_ASSETS = ("app_bundle", "app_executable", "runner")
def unique(pairs):
    value = {}
    for key, item in pairs:
        if key in value: raise ValueError(f"duplicate verified-input field: {key}")
        value[key] = item
    return value

def readonly(path: Path) -> bool:
    return path.lstat().st_mode & 0o222 == 0

def validate_tree(root: Path) -> None:
    if not root.is_dir() or root.is_symlink() or not readonly(root):
        raise ValueError("lane vTPM source is missing, unsafe, or writable")
    entries = list(root.rglob("*"))
    if len(entries) > 1_024: raise ValueError("lane vTPM source is oversized")
    for item in entries:
        mode = item.lstat().st_mode
        if stat.S_ISLNK(mode) or not (stat.S_ISREG(mode) or stat.S_ISDIR(mode)) or not readonly(item):
            raise ValueError("lane vTPM source contains an unsafe or writable entry")

def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True); parser.add_argument("--verified", type=Path, required=True)
    parser.add_argument("--job-id", required=True); parser.add_argument("--commit", required=True)
    parser.add_argument("--mode", choices=("pilot", "release"), required=True); parser.add_argument("--lane", type=int, required=True)
    parser.add_argument("--nonce", required=True); parser.add_argument("--lane-root", type=Path, required=True)
    args = parser.parse_args()
    verified = json.loads(args.verified.read_text(encoding="utf-8"), object_pairs_hook=unique)
    assets = verified.get("assets")
    if verified.get("verified") is not True or not isinstance(assets, dict) or verified.get("campaign_mode") != args.mode:
        raise ValueError("import inputs were not verified for this campaign")
    root = args.lane_root; raw_root = str(root)
    if not raw_root.startswith(("/tmp/bridgevm-import-e2e-", "/private/tmp/bridgevm-import-e2e-")) or raw_root != os.path.normpath(raw_root):
        raise ValueError("lane root is outside /tmp/bridgevm-import-e2e-*")
    inputs = root / "inputs"; disk = inputs / "windows.raw"; variables = inputs / "vars.fd"; vtpm = inputs / "vtpm"; package = inputs / "vtpm-recovery.json"; code = inputs / "vtpm-recovery-code.txt"
    if not root.is_dir() or root.is_symlink() or set(item.name for item in root.iterdir()) != {"inputs"}:
        raise ValueError("lane root must contain only its prepared inputs")
    if not disk.is_file() or disk.is_symlink() or not readonly(disk) or MANIFEST.file_hash(disk) != assets["source_disk"]["sha256"]:
        raise ValueError("lane disk clone is missing, mutable, or unauthenticated")
    if not variables.is_file() or variables.is_symlink() or not readonly(variables) or variables.stat().st_size != 64 * 1024 * 1024 or MANIFEST.file_hash(variables) != assets["source_vars"]["sha256"]:
        raise ValueError("lane vars clone is missing, mutable, malformed, or unauthenticated")
    for key, candidate in (("source_vtpm_package", package), ("source_vtpm_code", code)):
        if not candidate.is_file() or candidate.is_symlink() or not readonly(candidate) or MANIFEST.file_hash(candidate) != assets[key]["sha256"]: raise ValueError(f"lane {key} is missing, mutable, or unauthenticated")
    validate_tree(vtpm)
    if MANIFEST.tree_hash(vtpm, allow_symlinks=False) != assets["source_vtpm"]["sha256"]:
        raise ValueError("lane vTPM clone is unauthenticated")
    prefix = args.nonce[:12]; vm_name = f"BridgeVM A9 Import Lane {args.lane} {prefix}"
    vm_slug = f"bridgevm-a9-import-lane-{args.lane}-{prefix}"
    library = root / "library"; bundle = library / vm_slug / "bundle"
    request = {
        "schema_version": "bridgevm.windows-hvf-import-product-e2e-request.v1",
        "job_id": args.job_id, "commit": args.commit, "campaign_mode": args.mode,
        "lane": args.lane, "nonce": args.nonce, "vm_name": vm_name, "vm_slug": vm_slug,
        "three_d_injection": False,
        **{f"{key}_path": assets[key]["path"] for key in APP_ASSETS},
        "source_disk_path": str(disk), "source_vars_path": str(variables), "source_vtpm_path": str(vtpm), "source_vtpm_package_path": str(package), "source_vtpm_code_path": str(code),
        "lane_root": raw_root, "library_root_path": str(library), "share_path": str(root / "share"),
        "disk_path": str(bundle / "disks/hvf-target.raw"), "vars_path": str(bundle / "metadata/hvf-vars.fd"),
        "vtpm_state_path": str(bundle / "metadata/vtpm"),
        "snapshot_path": str(bundle / "metadata/snapshots/latest.snapshot"),
        "guest_evidence_path": str(bundle / "metadata/product-e2e-guest-evidence.json"),
    }
    with args.out.open("x", encoding="utf-8") as output:
        json.dump(request, output, indent=2, sort_keys=True); output.write("\n")
    return 0

if __name__ == "__main__": raise SystemExit(main())
