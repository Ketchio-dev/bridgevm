"""One T17 lane's request and vTPM state, read under T17Contract's identity and fixed-path rules.

Nothing here runs the app or writes. A Refusal's text names no private path,
so the tier may log it as written.
"""
from __future__ import annotations

import os
from pathlib import Path
import re
import stat

import product_e2e_work as WORK
from product_e2e_json_snapshot import JsonSnapshot

SCHEMA = "bridgevm.windows-hvf-3d-off-product-e2e-request.v2"
FIELDS = frozenset("""schema_version job_id commit campaign_mode lane nonce vm_name vm_slug three_d_injection
    app_bundle_path app_executable_path runner_path firmware_path secure_boot_policy_path iso_path
    bundled_vars_seed_path guest_payload_path guest_payload_manifest_path work_parent work_parent_identity
    work_identity lane_root library_root_path share_path disk_path vars_path vtpm_state_path snapshot_path
    secure_boot_receipt_path guest_evidence_path""".split())
BUNDLE = {"disk_path": "disks/hvf-target.raw", "vars_path": "metadata/hvf-vars.fd", "vtpm_state_path": "metadata/vtpm",
          "snapshot_path": "metadata/snapshots/latest.snapshot",
          "secure_boot_receipt_path": "metadata/secure-boot-provisioning.json",
          "guest_evidence_path": "metadata/product-e2e-guest-evidence.json"}
PATTERNS = {"job_id": re.compile(WORK.JOB + r"\Z"), "commit": re.compile(r"[0-9a-f]{40}\Z"),
            "nonce": re.compile(r"[0-9a-f]{64}\Z")}


class Refusal(ValueError):
    pass


def bounded_json(path: Path, what: str) -> object:
    try:
        return JsonSnapshot.read(path).value
    except (OSError, ValueError) as error:
        raise Refusal(f"{what} is not one bounded regular JSON file") from error


def matches(request: dict, field: str) -> bool:
    return type(request[field]) is str and PATTERNS[field].match(request[field]) is not None


def lane_request(work: Path, lane: Path, ordinal: int, mode: str, bundle: Path, executable: Path) -> dict | None:
    """Apply T17Contract's identity and fixed-path rules to the lane's own request, or refuse."""
    path = lane / "request.json"
    if not os.path.lexists(path):
        if os.listdir(lane):
            raise Refusal(f"lane {ordinal} holds files but no request")
        return None
    request = bounded_json(path, f"lane {ordinal} request")
    if not isinstance(request, dict) or set(request) != FIELDS:
        raise Refusal(f"lane {ordinal} request does not have the exact T17 fields")
    prefix = request["nonce"][:12] if matches(request, "nonce") else None
    if not (prefix and request["schema_version"] == SCHEMA and matches(request, "job_id") and matches(request, "commit")
            and request["campaign_mode"] == mode and type(request["lane"]) is int and request["lane"] == ordinal
            and request["three_d_injection"] is False and request["vm_name"] == f"BridgeVM T17 Lane {ordinal} {prefix}"
            and request["vm_slug"] == f"bridgevm-t17-lane-{ordinal}-{prefix}"):
        raise Refusal(f"lane {ordinal} request identity is not its T17 lane slug")
    library = lane / "library"
    fixed = {"lane_root": lane, "library_root_path": library, "share_path": lane / "share",
             "app_bundle_path": bundle, "app_executable_path": executable,
             **{field: library / request["vm_slug"] / "bundle.vmbridge" / name for field, name in BUNDLE.items()}}
    if any(request[field] != str(value) for field, value in fixed.items()):
        raise Refusal(f"lane {ordinal} request names a path outside its own lane")
    try:
        bound = WORK.validate(request, "e2e")
    except ValueError:
        bound = None
    if bound != work:
        raise Refusal(f"lane {ordinal} request is not bound to this work tree")
    return request


def only_registration(library: Path, slug: str, ordinal: int) -> None:
    """Refuse a lane library that holds a VM whose key this lane cannot account for."""
    for name in os.listdir(library):
        mode = os.lstat(library / name).st_mode
        source_cache = name == "Derived" and stat.S_ISDIR(mode) and not os.path.lexists(library / name / "vm.json")
        if name != slug and not stat.S_ISREG(mode) and not source_cache:
            raise Refusal(f"lane {ordinal} library holds a VM registration other than its slug")


def lane_state(request: dict, ordinal: int) -> tuple[Path | None, bool]:
    """Return the lane's vTPM state directory, or None when absent, and whether it holds data."""
    library, slug = Path(request["library_root_path"]), request["vm_slug"]
    chain = [library, library / slug, library / slug / "bundle.vmbridge", library / slug / "bundle.vmbridge/metadata",
             Path(request["vtpm_state_path"])]
    for index, path in enumerate(chain):
        try:
            mode = os.lstat(path).st_mode
        except FileNotFoundError:
            return None, False
        if not stat.S_ISDIR(mode):
            raise Refusal(f"lane {ordinal} vTPM state path has a link or non-directory component")
        if index == 0:
            only_registration(library, slug, ordinal)
    return chain[-1], bool(os.listdir(chain[-1]))
