"""Path-local synthetic T17/T19 records for identity and request-seal checks."""
from __future__ import annotations

import importlib.util
import json
from pathlib import Path
import subprocess
import sys
from product_e2e_selected_fixtures import package_pair_helper

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/live-gates"))


def module(name: str, script: str):
    spec = importlib.util.spec_from_file_location(name, ROOT / "scripts/live-gates" / script)
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


T17 = module("t17_identity_writer", "write-windows-product-e2e-receipt.py")
T19 = module("t19_identity_writer", "write-windows-import-product-e2e-receipt.py")
JOB, COMMIT, NONCE = "integrity-fixture", "b" * 40, "a" * 64


def lane(writer) -> dict:
    value = {"schema_version": writer.LANE_SCHEMA, "job_id": JOB, "commit": COMMIT,
             "campaign_mode": "pilot", "lane": 1, "nonce": NONCE, "three_d_injection": False,
             "ui_frontend_automated": True, "failure_code": "none", "failure_detail": "",
             "cleanup_verified": True, **dict.fromkeys(writer.LANE_STAGES, True),
             **dict.fromkeys(writer.LANE_HASHES, "c" * 64)}
    if writer is T17:
        value["installer_source_path"] = "/private/tmp/synthetic-source.raw"
    return value


def stamp(writer, result: Path) -> dict:
    schema = ("bridgevm.windows-hvf-3d-off-product-e2e-host-stamp.v1" if writer is T17
              else "bridgevm.windows-hvf-import-product-e2e-host-stamp.v1")
    return {"schema_version": schema, "job_id": JOB, "commit": COMMIT, "lane": 1,
            "nonce": NONCE, "request_sha256": "d" * 64, "result_sha256": writer.digest(result)}

def import_request(root: Path) -> tuple[Path, Path]:
    inputs = root / "inputs"
    inputs.mkdir(parents=True)
    (inputs / "windows.raw").write_bytes(b"synthetic disk")
    (inputs / "vars.fd").write_bytes(b"synthetic vars")
    (inputs / "vtpm").mkdir()
    (inputs / "vtpm/state").write_bytes(b"synthetic state")
    (inputs / "vtpm-recovery.json").write_bytes(b"synthetic package")
    (inputs / "vtpm-recovery-code.txt").write_bytes(b"synthetic code")
    slug = f"bridgevm-a9-import-lane-1-{NONCE[:12]}"
    bundle = root / "library" / slug / "bundle"
    paths = {"app_bundle_path": root / "Fixture.app", "app_executable_path": root / "Fixture.app/Contents/MacOS/BridgeVMControl",
             "runner_path": root / "Fixture.app/Contents/Resources/target/release/hvf-runner",
             "source_disk_path": inputs / "windows.raw", "source_vars_path": inputs / "vars.fd",
             "source_vtpm_path": inputs / "vtpm", "source_vtpm_package_path": inputs / "vtpm-recovery.json",
             "source_vtpm_code_path": inputs / "vtpm-recovery-code.txt", "lane_root": root,
             "library_root_path": root / "library", "share_path": root / "share", "disk_path": bundle / "disks/hvf-target.raw",
             "vars_path": bundle / "metadata/hvf-vars.fd", "vtpm_state_path": bundle / "metadata/vtpm",
             "snapshot_path": bundle / "metadata/snapshots/latest.snapshot", "guest_evidence_path": bundle / "metadata/product-e2e-guest-evidence.json"}
    request = {"schema_version": "bridgevm.windows-hvf-import-product-e2e-request.v1", "job_id": JOB,
               "commit": COMMIT, "campaign_mode": "pilot", "lane": 1, "nonce": NONCE,
               "vm_name": f"BridgeVM A9 Import Lane 1 {NONCE[:12]}", "vm_slug": slug,
               "three_d_injection": False, **{key: str(value) for key, value in paths.items()}}
    request_path, result_path = root / "request.json", root.parent / "result.json"
    request_path.write_text(json.dumps(request)); package_pair_helper(paths["app_bundle_path"], ROOT)
    subprocess.run([sys.executable, str(ROOT / "tests/fixtures/fake-windows-import-product-e2e-helper.py"),
                    "--windows-import-product-e2e", "--request", str(request_path), "--result", str(result_path)],
                   check=True, capture_output=True)
    return request_path, result_path
