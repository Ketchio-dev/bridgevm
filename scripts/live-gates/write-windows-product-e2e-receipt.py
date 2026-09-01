#!/usr/bin/env python3
"""Derive the exact public T17 receipt from authenticated inputs and lane results."""
from __future__ import annotations
import argparse, hashlib, importlib.util, json, platform, subprocess, sys
from datetime import datetime, timezone
from pathlib import Path
import windows_product_e2e_artifacts as ARTIFACTS

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("product_receipt_verifier", ROOT / "scripts/verify-windows-product-e2e-receipt.py")
if SPEC is None or SPEC.loader is None:
    raise RuntimeError("cannot load product receipt verifier")
VERIFIER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(VERIFIER)
LANE_SCHEMA = "bridgevm.windows-hvf-3d-off-product-e2e-lane.v2"
LANE_STAGES = tuple(field.removesuffix("_passes") for field in VERIFIER.STAGE_FIELDS)
LANE_HASHES = ("installer_source_sha256", "final_disk_sha256", "final_vars_sha256", "secure_boot_receipt_sha256", "guest_evidence_sha256")
LANE_FAILURE_CODES = frozenset({"none", "invalid-request", "missing-guest-payload", "accessibility-untrusted", "app-launch-failed", "ui-element-missing", "input-selection-failed", "vm-creation-failed", "installer-failed", "guest-evidence-missing", "snapshot-unavailable", "cleanup-failed", "canceled", "internal-error"})
LANE_KEYS = frozenset({"schema_version", "job_id", "commit", "campaign_mode", "lane", "nonce", "three_d_injection", "ui_frontend_automated", "failure_code", "cleanup_verified", "installer_source_path", *LANE_STAGES, *LANE_HASHES})
REQUEST_PATHS = ("app_bundle_path", "app_executable_path", "runner_path", "firmware_path", "secure_boot_policy_path", "iso_path", "bundled_vars_seed_path", "guest_payload_path", "guest_payload_manifest_path", "lane_root", "library_root_path", "share_path", "disk_path", "vars_path", "vtpm_state_path", "snapshot_path", "secure_boot_receipt_path", "guest_evidence_path")
REQUEST_KEYS = frozenset({"schema_version", "job_id", "commit", "campaign_mode", "lane", "nonce", "three_d_injection", "vm_name", "vm_slug", *REQUEST_PATHS})
STAMP_KEYS = frozenset({"schema_version", "job_id", "commit", "lane", "nonce", "request_sha256", "result_sha256"})

def unique_object(pairs: list[tuple[str, object]]) -> dict:
    result: dict[str, object] = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"duplicate field: {key}")
        result[key] = value
    return result

def load_json(path: Path) -> object:
    if not path.is_file() or path.is_symlink() or path.stat().st_size == 0 or path.stat().st_size > 1024 * 1024:
        raise ValueError(f"unsafe or oversized JSON input: {path.name}")
    return json.loads(path.read_text(encoding="utf-8"), object_pairs_hook=unique_object)

def digest(path: Path) -> str:
    if not path.is_file() or path.is_symlink():
        return "absent"
    value = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            value.update(chunk)
    return value.hexdigest()

def aggregate(values: list[str]) -> str:
    if not values:
        return "absent"
    if len(values) == 1:
        return values[0]
    body = "".join(f"{index + 1}\t{value}\n" for index, value in enumerate(values))
    return hashlib.sha256(body.encode()).hexdigest()

def host_value(command: list[str], fallback: str) -> str:
    try:
        value = subprocess.check_output(command, text=True, stderr=subprocess.DEVNULL).strip()
    except (OSError, subprocess.CalledProcessError):
        value = ""
    return value or fallback

def lane(path: Path, *, job_id: str, commit: str, mode: str, ordinal: int, stamp: Path | None = None) -> dict:
    value = load_json(path)
    if not isinstance(value, dict) or frozenset(value) != LANE_KEYS:
        raise ValueError(f"lane {ordinal} has missing or unknown fields")
    fixed = {"schema_version": LANE_SCHEMA, "job_id": job_id, "commit": commit, "campaign_mode": mode, "lane": ordinal, "three_d_injection": False}
    if any(value.get(key) != expected for key, expected in fixed.items()):
        raise ValueError(f"lane {ordinal} identity or 3D policy differs")
    if not isinstance(value["nonce"], str) or not VERIFIER.SHA256.fullmatch(value["nonce"]):
        raise ValueError(f"lane {ordinal} nonce is invalid")
    source_prepared = value["source_prepared"] is True
    if not isinstance(value["installer_source_path"], str) or (value["installer_source_path"] == "absent") == source_prepared or (source_prepared and not value["installer_source_path"].startswith("/")):
        raise ValueError(f"lane {ordinal} installer source path is invalid")
    previous = True
    for stage in LANE_STAGES:
        current = value[stage]
        if not isinstance(current, bool) or (current and not previous):
            raise ValueError(f"lane {ordinal} stage sequence is invalid at {stage}")
        previous = current
    if value["failure_code"] not in LANE_FAILURE_CODES or (value["failure_code"] == "none") != all(value[stage] for stage in LANE_STAGES):
        raise ValueError(f"lane {ordinal} failure code contradicts its stages")
    if not isinstance(value["cleanup_verified"], bool) or not isinstance(value["ui_frontend_automated"], bool) or (all(value[stage] for stage in LANE_STAGES) and value["ui_frontend_automated"] is not True):
        raise ValueError(f"lane {ordinal} cleanup result is invalid")
    for field in LANE_HASHES:
        if not isinstance(value[field], str) or not VERIFIER.SHA256.fullmatch(value[field]):
            raise ValueError(f"lane {ordinal} {field} is not a SHA-256")
    if stamp is not None:
        seal = load_json(stamp)
        fixed_stamp = {"schema_version": "bridgevm.windows-hvf-3d-off-product-e2e-host-stamp.v1", "job_id": job_id, "commit": commit, "lane": ordinal, "nonce": value["nonce"], "result_sha256": digest(path)}
        if not isinstance(seal, dict) or frozenset(seal) != STAMP_KEYS or any(seal.get(key) != expected for key, expected in fixed_stamp.items()) or not VERIFIER.SHA256.fullmatch(str(seal.get("request_sha256", ""))):
            raise ValueError(f"lane {ordinal} authentication stamp is invalid")
    return value

def authenticate(request_path: Path, result_path: Path, stamp_path: Path, *, job_id: str, commit: str, mode: str, ordinal: int) -> None:
    result = lane(result_path, job_id=job_id, commit=commit, mode=mode, ordinal=ordinal)
    if not all(result[stage] for stage in LANE_STAGES) or not result["cleanup_verified"]:
        raise ValueError(f"lane {ordinal} did not prove every fixed product stage and cleanup")
    request = load_json(request_path)
    nonce_prefix = result["nonce"][:12]
    vm_name = f"BridgeVM T17 Lane {ordinal} {nonce_prefix}"
    vm_slug = f"bridgevm-t17-lane-{ordinal}-{nonce_prefix}"
    fixed = {"schema_version": "bridgevm.windows-hvf-3d-off-product-e2e-request.v2", "job_id": job_id, "commit": commit, "campaign_mode": mode, "lane": ordinal, "nonce": result["nonce"], "three_d_injection": False, "vm_name": vm_name, "vm_slug": vm_slug}
    if not isinstance(request, dict) or frozenset(request) != REQUEST_KEYS or any(request.get(key) != expected for key, expected in fixed.items()):
        raise ValueError(f"lane {ordinal} request is malformed or unbound")
    for field in REQUEST_PATHS:
        if not isinstance(request[field], str) or not request[field].startswith("/"):
            raise ValueError(f"lane {ordinal} request path {field} is invalid")
    ARTIFACTS.authenticate(request, result, ordinal)
    stamp = {"schema_version": "bridgevm.windows-hvf-3d-off-product-e2e-host-stamp.v1", "job_id": job_id, "commit": commit, "lane": ordinal, "nonce": result["nonce"], "request_sha256": digest(request_path), "result_sha256": digest(result_path)}
    with stamp_path.open("x", encoding="utf-8") as output:
        json.dump(stamp, output, indent=2, sort_keys=True); output.write("\n")

def build(args: argparse.Namespace) -> dict:
    expected = 3 if args.mode == "release" else 1
    verified: dict = {}
    try:
        loaded = load_json(args.verified)
        if isinstance(loaded, dict):
            verified = loaded
    except (OSError, UnicodeError, json.JSONDecodeError, ValueError):
        pass
    assets = verified.get("assets", {}) if isinstance(verified.get("assets"), dict) else {}
    def asset(key: str) -> str:
        item = assets.get(key, {})
        value = item.get("sha256", "absent") if isinstance(item, dict) else "absent"
        return value if isinstance(value, str) and VERIFIER.SHA256.fullmatch(value) else "absent"

    results: list[dict] = []
    for ordinal in range(1, args.attempts + 1):
        try:
            results.append(lane(args.private / f"lane-{ordinal}-result.json", job_id=args.job_id, commit=args.commit, mode=args.mode, ordinal=ordinal, stamp=args.private / f"lane-{ordinal}-authenticated.json"))
        except (OSError, UnicodeError, json.JSONDecodeError, ValueError):
            continue
    successful = [result for result in results if all(result[stage] for stage in LANE_STAGES) and result["cleanup_verified"]]
    stages = {f"{stage}_passes": sum(bool(result[stage]) for result in results) for stage in LANE_STAGES}
    final_hashes = {field: aggregate([result[field] for result in successful]) for field in LANE_HASHES}
    started = datetime.strptime(args.started_at, "%Y-%m-%dT%H:%M:%SZ").replace(tzinfo=timezone.utc)
    finished = datetime.now(timezone.utc).replace(microsecond=0)
    if finished < started:
        finished = started
    run_count, passes = args.attempts, len(successful)
    hashes = {
        "input_manifest_sha256": digest(args.input_manifest),
        "app_artifact_sha256": asset("app_bundle"),
        "app_executable_sha256": asset("app_executable"),
        "helper_sha256": asset("product_helper"),
        "runner_sha256": asset("runner"),
        "firmware_sha256": asset("firmware"),
        "secure_boot_policy_sha256": asset("secure_boot_policy"),
        "iso_sha256": asset("iso"),
        "bundled_vars_seed_sha256": asset("bundled_vars_seed"),
        "installer_source_sha256": final_hashes["installer_source_sha256"],
        "final_disk_sha256": final_hashes["final_disk_sha256"],
        "final_vars_sha256": final_hashes["final_vars_sha256"],
        "secure_boot_receipt_sha256": final_hashes["secure_boot_receipt_sha256"],
        "guest_evidence_sha256": final_hashes["guest_evidence_sha256"],
    }
    completed = args.outcome == "completed"
    product_model = len(results) == run_count and run_count > 0
    ui_frontend = len(successful) == run_count and run_count > 0 and all(result["ui_frontend_automated"] for result in successful)
    preliminary = args.valid and completed and run_count == expected and passes == expected and stages[VERIFIER.STAGE_FIELDS[-1]] == expected and product_model and ui_frontend and args.cleanup and all(value != "absent" for value in hashes.values())
    receipt = {
        "schema_version": VERIFIER.SCHEMA, "gate_id": VERIFIER.GATE_ID, "criterion": "A9", "tier": VERIFIER.TIER,
        "job_id": args.job_id, "tested_commit": args.commit, "commit": args.commit, "hosted_ci_commit": args.commit,
        "campaign_mode": args.mode, "artifact_signing_class": args.signing_class,
        "clean_machine": False, "ui_frontend_automated": ui_frontend, "product_model_automated": product_model,
        "three_d_injection": False, "worker_cleanup_verified": args.cleanup,
        "hosted_ci_green": False, "security_ci_green": False, "valid": args.valid,
        "expected_runs": expected, "run_count": run_count, "passes": passes, "failures": run_count - passes,
        "elapsed_ms": int((finished - started).total_seconds() * 1000), "failure_code": args.failure_code,
        "outcome": args.outcome, "pass": preliminary, "claim_eligible": False,
        "criterion_pass": False, "capability_promotion": False,
        "host_model": host_value(["sysctl", "-n", "hw.model"], platform.machine() or "unknown-host"),
        "macos_version": host_value(["sw_vers", "-productVersion"], platform.mac_ver()[0] or "unknown-macos"),
        "started_at": started.strftime("%Y-%m-%dT%H:%M:%SZ"), "finished_at": finished.strftime("%Y-%m-%dT%H:%M:%SZ"),
        "hosted_ci_run_id": "absent", "security_ci_run_id": "absent", **hashes, **stages,
    }
    VERIFIER.validate(receipt, expected_commit=args.commit)
    return receipt

def main() -> int:
    if sys.argv[1:2] == ["--check-lane"]:
        check = argparse.ArgumentParser()
        check.add_argument("--check-lane", type=Path, required=True)
        check.add_argument("--job-id", required=True)
        check.add_argument("--commit", required=True)
        check.add_argument("--mode", choices=("pilot", "release"), required=True)
        check.add_argument("--ordinal", type=int, required=True)
        check.add_argument("--request", type=Path, required=True)
        check.add_argument("--stamp", type=Path, required=True)
        values = check.parse_args()
        try:
            authenticate(values.request, values.check_lane, values.stamp, job_id=values.job_id, commit=values.commit, mode=values.mode, ordinal=values.ordinal)
        except (OSError, UnicodeError, json.JSONDecodeError, ValueError) as error:
            print(f"invalid T17 lane result: {error}", file=sys.stderr)
            return 1
        return 0
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--private", type=Path, required=True)
    parser.add_argument("--input-manifest", type=Path, required=True)
    parser.add_argument("--verified", type=Path, required=True)
    parser.add_argument("--job-id", required=True)
    parser.add_argument("--commit", required=True)
    parser.add_argument("--mode", choices=("pilot", "release"), required=True)
    parser.add_argument("--attempts", type=int, default=0)
    parser.add_argument("--started-at", required=True)
    parser.add_argument("--outcome", choices=tuple(VERIFIER.OUTCOMES), required=True)
    parser.add_argument("--failure-code", choices=tuple(VERIFIER.FAILURE_CODES), required=True)
    parser.add_argument("--signing-class", choices=("development-ad-hoc", "developer-id-notarized"), default="development-ad-hoc")
    parser.add_argument("--cleanup", action="store_true")
    parser.add_argument("--valid", action="store_true")
    args = parser.parse_args()
    if args.attempts < 0 or args.attempts > (3 if args.mode == "release" else 1):
        parser.error("attempt count is outside the fixed campaign")
    try:
        receipt = build(args)
        args.out.parent.mkdir(parents=True, exist_ok=True)
        with args.out.open("x", encoding="utf-8") as output:
            json.dump(receipt, output, indent=2, sort_keys=True)
            output.write("\n")
    except (OSError, UnicodeError, json.JSONDecodeError, ValueError, VERIFIER.ReceiptError) as error:
        print(f"refusing T17 receipt: {error}", file=sys.stderr)
        return 1
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
