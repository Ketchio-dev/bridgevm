#!/usr/bin/env python3
"""Host-authenticate private T17 product artifacts without publishing their paths."""
from __future__ import annotations
import hashlib, json, re
from pathlib import Path
import windows_product_e2e_guest_evidence as GUEST

SHA256 = re.compile(r"^[0-9a-f]{64}$")
STAGES = ("artifact_preflight", "vm_created", "source_prepared", "windows_installed", "secure_boot_provisioned", "first_ready", "keyboard_pointer", "clipboard", "folder_share", "network", "audio", "first_shutdown", "snapshot_restore", "second_ready", "second_shutdown")

def unique(pairs: list[tuple[str, object]]) -> dict:
    result: dict[str, object] = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"duplicate JSON field: {key}")
        result[key] = value
    return result

def load(path: Path) -> dict:
    if not path.is_file() or path.is_symlink() or path.stat().st_size == 0 or path.stat().st_size > 1024 * 1024:
        raise ValueError(f"unsafe product JSON: {path.name}")
    value = json.loads(path.read_text(encoding="utf-8"), object_pairs_hook=unique)
    if not isinstance(value, dict):
        raise ValueError(f"product JSON is not an object: {path.name}")
    return value

def digest(path: Path) -> str:
    if not path.is_file() or path.is_symlink():
        return "absent"
    value = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            value.update(chunk)
    return value.hexdigest()

def snapshot_files(root: Path, vm_slug: str) -> tuple[Path, Path, Path]:
    if not root.is_dir() or root.is_symlink():
        raise ValueError("snapshot is not a non-symlink directory")
    entries = {item.name: item for item in root.iterdir()}
    if set(entries) != {"disk.raw", "vars.fd", "manifest.json"}:
        raise ValueError("snapshot directory does not contain its exact pair")
    disk, variables, receipt = entries["disk.raw"], entries["vars.fd"], entries["manifest.json"]
    if any(not item.is_file() or item.is_symlink() for item in (disk, variables, receipt)):
        raise ValueError("snapshot pair contains an unsafe entry")
    manifest = load(receipt)
    keys = {"format_version", "vm_id", "disk_bytes", "disk_sha256", "vars_bytes", "vars_sha256"}
    if set(manifest) != keys or manifest.get("format_version") != 1 or manifest.get("vm_id") != vm_slug:
        raise ValueError("snapshot manifest identity is invalid")
    for item, size_field, hash_field in ((disk, "disk_bytes", "disk_sha256"), (variables, "vars_bytes", "vars_sha256")):
        if manifest.get(size_field) != item.stat().st_size or manifest.get(hash_field) != digest(item):
            raise ValueError(f"snapshot manifest does not authenticate {item.name}")
    return disk, variables, receipt

def verify_secure_boot(request: dict) -> None:
    policy = load(Path(request["secure_boot_policy_path"]))
    receipt = load(Path(request["secure_boot_receipt_path"]))
    receipt_keys = {"schemaVersion", "policy", "sourceTag", "sourceCommit", "sourceAssetSha256", "firmwareFileName", "firmwareSha256", "firmwareEdk2Commit", "provisionedAt", "variables"}
    if set(receipt) != receipt_keys or receipt.get("schemaVersion") != 1:
        raise ValueError("Secure Boot receipt fields are invalid")
    source, firmware, variables = policy.get("source"), policy.get("firmware"), policy.get("variables")
    if not isinstance(source, dict) or not isinstance(firmware, dict) or not isinstance(variables, list):
        raise ValueError("Secure Boot policy is malformed")
    fixed = {"policy": policy.get("policy"), "sourceTag": source.get("tag"), "sourceCommit": source.get("commit"), "sourceAssetSha256": source.get("assetSha256"), "firmwareFileName": firmware.get("fileName"), "firmwareSha256": firmware.get("sha256"), "firmwareEdk2Commit": firmware.get("edk2Commit")}
    expected_variables = [{"name": item.get("name"), "vendorGuid": item.get("vendorGuid"), "attributes": item.get("attributes"), "payloadSha256": item.get("sha256")} for item in variables if isinstance(item, dict)]
    timestamp = receipt.get("provisionedAt")
    if any(receipt.get(key) != value for key, value in fixed.items()) or receipt.get("variables") != expected_variables or not isinstance(timestamp, str) or "T" not in timestamp or digest(Path(request["firmware_path"])) != receipt.get("firmwareSha256"):
        raise ValueError("Secure Boot receipt does not match its sealed policy and firmware")

def authenticate(request: dict, result: dict, ordinal: int) -> None:
    root = Path(request["lane_root"]); library = root / "library"; vm_slug = request["vm_slug"]
    bundle = library / vm_slug / "bundle.vmbridge"
    expected = {"library_root_path": library, "share_path": root / "share", "disk_path": bundle / "disks/hvf-target.raw", "vars_path": bundle / "metadata/hvf-vars.fd", "vtpm_state_path": bundle / "metadata/vtpm", "snapshot_path": bundle / "metadata/snapshots/latest.snapshot", "secure_boot_receipt_path": bundle / "metadata/secure-boot-provisioning.json", "guest_evidence_path": bundle / "metadata/product-e2e-guest-evidence.json"}
    if any(Path(request[field]) != path for field, path in expected.items()):
        raise ValueError(f"lane {ordinal} paths differ from the product bundle")
    directories = [root, library, Path(request["share_path"]), bundle, Path(request["vtpm_state_path"])]
    if any(not item.is_dir() or item.is_symlink() for item in directories):
        raise ValueError(f"lane {ordinal} product directory is missing or unsafe")
    config = load(library / vm_slug / "vm.json")
    if config.get("id") != vm_slug or config.get("name") != request["vm_name"] or config.get("bundlePath") != str(bundle) or config.get("installPending") is not False or config.get("experimental3DAllowed") is not False:
        raise ValueError(f"lane {ordinal} product VM identity or 3D policy differs")
    probe = Path(request["share_path"]) / f"t17-{request['nonce'][:12]}.txt"
    if not probe.is_file() or probe.is_symlink() or probe.read_text(encoding="utf-8") != f"bridgevm-t17-share-v1\n{request['nonce']}\n":
        raise ValueError(f"lane {ordinal} share is not nonce-bound")
    source = Path(result["installer_source_path"]); source_root = library / "Derived/WindowsInstallSources"
    if source.parent != source_root or not re.fullmatch(r"win11-[0-9a-f]{64}\.raw", source.name) or not source.is_file() or source.is_symlink():
        raise ValueError(f"lane {ordinal} installer source is outside the product cache")
    source_receipt = Path(str(source) + ".sha256"); source_hash = digest(source)
    if not source_receipt.is_file() or source_receipt.is_symlink() or source_receipt.read_text(encoding="ascii") != source_hash + "\n" or result["installer_source_sha256"] != source_hash or source_hash == digest(Path(request["iso_path"])):
        raise ValueError(f"lane {ordinal} installer source cache receipt is invalid")
    files = {"final_disk_sha256": "disk_path", "final_vars_sha256": "vars_path", "secure_boot_receipt_sha256": "secure_boot_receipt_path", "guest_evidence_sha256": "guest_evidence_path"}
    for hash_field, path_field in files.items():
        if digest(Path(request[path_field])) != result[hash_field]:
            raise ValueError(f"lane {ordinal} {hash_field} does not authenticate its file")
    snapshot_disk, snapshot_vars, snapshot_receipt = snapshot_files(Path(request["snapshot_path"]), vm_slug)
    if digest(snapshot_disk) != result["final_disk_sha256"] or digest(snapshot_vars) != result["final_vars_sha256"]:
        raise ValueError(f"lane {ordinal} restored media differs from its snapshot")
    verify_secure_boot(request); private_observations = GUEST.verify(request)
    vtpm_entries = list(Path(request["vtpm_state_path"]).rglob("*"))
    if not vtpm_entries or any(item.is_symlink() or (not item.is_file() and not item.is_dir()) for item in vtpm_entries):
        raise ValueError(f"lane {ordinal} vTPM state is missing or unsafe")
    state_files = [source, source_receipt, probe, library / vm_slug / "vm.json", *[Path(request[field]) for field in files.values() if field != "guest_evidence_path"], snapshot_disk, snapshot_vars, snapshot_receipt, *private_observations, *[item for item in vtpm_entries if item.is_file()]]
    input_files = [Path(request[field]) for field in ("runner_path", "firmware_path", "secure_boot_policy_path", "iso_path", "bundled_vars_seed_path", "guest_payload_manifest_path")]
    input_files.extend(item for root_path in (Path(request["app_bundle_path"]), Path(request["guest_payload_path"])) for item in root_path.rglob("*") if item.is_file())
    if len({(item.stat().st_dev, item.stat().st_ino) for item in state_files}) != len(state_files) or any(output.samefile(source_file) for output in state_files for source_file in input_files):
        raise ValueError(f"lane {ordinal} writable state is aliased to another artifact")
