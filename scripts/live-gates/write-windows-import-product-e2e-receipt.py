#!/usr/bin/env python3
"""Authenticate import lanes and derive the exact public T19 receipt."""
from __future__ import annotations
import argparse, hashlib, importlib.util, json, os, platform, stat, subprocess, sys
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("import_receipt_verifier", ROOT / "scripts/verify-windows-import-product-e2e-receipt.py")
VERIFIER = importlib.util.module_from_spec(SPEC); SPEC.loader.exec_module(VERIFIER)
LANE_SCHEMA = "bridgevm.windows-hvf-import-product-e2e-lane.v1"
LANE_STAGES = tuple(field.removesuffix("_passes") for field in VERIFIER.STAGE_FIELDS)
LANE_HASHES = VERIFIER.HASH_FIELDS[5:]
LANE_FAILURES = {"none", "invalid-import-request", "accessibility-untrusted", "app-launch-failed",
    "ui-element-missing", "input-selection-failed", "import-registration-invalid", "import-media-mismatch",
    "source-media-mutated", "import-media-invalid", "guest-evidence-missing", "snapshot-unavailable",
    "cleanup-failed", "canceled", "internal-error"}
LANE_KEYS = frozenset({"schema_version", "job_id", "commit", "campaign_mode", "lane", "nonce",
    "three_d_injection", "ui_frontend_automated", "failure_code", "failure_detail", "cleanup_verified",
    *LANE_STAGES, *LANE_HASHES})
REQUEST_PATHS = ("app_bundle_path", "app_executable_path", "runner_path", "source_disk_path",
    "source_vars_path", "source_vtpm_path", "lane_root", "library_root_path", "share_path", "disk_path",
    "vars_path", "vtpm_state_path", "snapshot_path", "guest_evidence_path")
REQUEST_KEYS = frozenset({"schema_version", "job_id", "commit", "campaign_mode", "lane", "nonce",
    "vm_name", "vm_slug", "three_d_injection", *REQUEST_PATHS})
STAMP_KEYS = frozenset({"schema_version", "job_id", "commit", "lane", "nonce", "request_sha256", "result_sha256"})

def unique(pairs):
    value = {}
    for key, item in pairs:
        if key in value: raise ValueError(f"duplicate field: {key}")
        value[key] = item
    return value

def load(path: Path):
    if not path.is_file() or path.is_symlink() or not 0 < path.stat().st_size <= 1024 * 1024:
        raise ValueError(f"unsafe JSON: {path.name}")
    return json.loads(path.read_text(encoding="utf-8"), object_pairs_hook=unique)

def digest(path: Path) -> str:
    if not path.is_file() or path.is_symlink(): return "absent"
    value = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""): value.update(chunk)
    return value.hexdigest()

def tree_digest(root: Path) -> str:
    if not root.is_dir() or root.is_symlink(): raise ValueError("unsafe vTPM tree")
    records = []
    entries = sorted(root.rglob("*"), key=lambda item: item.relative_to(root).as_posix())
    if len(entries) > 1024: raise ValueError("oversized vTPM tree")
    for item in entries:
        relative = item.relative_to(root).as_posix(); mode = item.lstat().st_mode
        if relative == ".lock": continue
        if stat.S_ISLNK(mode): raise ValueError("vTPM tree contains a symlink")
        if stat.S_ISDIR(mode): records.append(f"D\t{relative}\n")
        elif stat.S_ISREG(mode): records.append(f"F\t{relative}\t{digest(item)}\n")
        else: raise ValueError("vTPM tree contains an unsupported entry")
    return hashlib.sha256("".join(sorted(records)).encode()).hexdigest()

def aggregate(values):
    if not values: return "absent"
    if len(values) == 1: return values[0]
    return hashlib.sha256("".join(f"{i+1}\t{value}\n" for i, value in enumerate(values)).encode()).hexdigest()

def lane(path: Path, job: str, commit: str, mode: str, ordinal: int, stamp: Path | None = None):
    value = load(path)
    fixed = {"schema_version": LANE_SCHEMA, "job_id": job, "commit": commit, "campaign_mode": mode,
             "lane": ordinal, "three_d_injection": False}
    if not isinstance(value, dict) or frozenset(value) != LANE_KEYS or any(value.get(k) != v for k, v in fixed.items()):
        raise ValueError(f"lane {ordinal} identity differs")
    if not isinstance(value["nonce"], str) or not VERIFIER.SHA256.fullmatch(value["nonce"]):
        raise ValueError(f"lane {ordinal} nonce is invalid")
    previous = True
    for stage in LANE_STAGES:
        current = value[stage]
        if not isinstance(current, bool) or current and not previous: raise ValueError(f"lane {ordinal} stage order differs")
        previous = current
    complete = all(value[stage] for stage in LANE_STAGES)
    if value["failure_code"] not in LANE_FAILURES or (value["failure_code"] == "none") != (complete and value["cleanup_verified"] is True):
        raise ValueError(f"lane {ordinal} failure code contradicts evidence")
    if not isinstance(value["failure_detail"], str) or len(value["failure_detail"].encode()) > 4096:
        raise ValueError(f"lane {ordinal} failure detail is invalid")
    if not isinstance(value["ui_frontend_automated"], bool) or not isinstance(value["cleanup_verified"], bool):
        raise ValueError(f"lane {ordinal} booleans are invalid")
    for field in LANE_HASHES:
        if not isinstance(value[field], str) or not VERIFIER.SHA256.fullmatch(value[field]):
            raise ValueError(f"lane {ordinal} {field} is invalid")
    if stamp is not None:
        seal = load(stamp); expected = {"schema_version": "bridgevm.windows-hvf-import-product-e2e-host-stamp.v1",
            "job_id": job, "commit": commit, "lane": ordinal, "nonce": value["nonce"], "result_sha256": digest(path)}
        if not isinstance(seal, dict) or frozenset(seal) != STAMP_KEYS or any(seal.get(k) != v for k, v in expected.items()) or not VERIFIER.SHA256.fullmatch(str(seal.get("request_sha256", ""))):
            raise ValueError(f"lane {ordinal} stamp is invalid")
    return value

def authenticate(request_path: Path, result_path: Path, stamp: Path, job: str, commit: str, mode: str, ordinal: int):
    result = lane(result_path, job, commit, mode, ordinal)
    if result["failure_code"] != "none": raise ValueError(f"lane {ordinal} is incomplete")
    request = load(request_path); prefix = result["nonce"][:12]
    fixed = {"schema_version": "bridgevm.windows-hvf-import-product-e2e-request.v1", "job_id": job,
        "commit": commit, "campaign_mode": mode, "lane": ordinal, "nonce": result["nonce"],
        "three_d_injection": False, "vm_name": f"BridgeVM A9 Import Lane {ordinal} {prefix}",
        "vm_slug": f"bridgevm-a9-import-lane-{ordinal}-{prefix}"}
    if not isinstance(request, dict) or frozenset(request) != REQUEST_KEYS or any(request.get(k) != v for k, v in fixed.items()):
        raise ValueError(f"lane {ordinal} request is malformed")
    root = Path(request["lane_root"]); bundle = root / "library" / fixed["vm_slug"] / "bundle"
    expected = {"source_disk_path": root/"inputs/windows.raw", "source_vars_path": root/"inputs/vars.fd",
        "source_vtpm_path": root/"inputs/vtpm", "library_root_path": root/"library", "share_path": root/"share",
        "disk_path": bundle/"disks/hvf-target.raw", "vars_path": bundle/"metadata/hvf-vars.fd",
        "vtpm_state_path": bundle/"metadata/vtpm", "snapshot_path": bundle/"metadata/snapshots/latest.snapshot",
        "guest_evidence_path": bundle/"metadata/product-e2e-guest-evidence.json"}
    if not str(root).startswith("/tmp/bridgevm-import-e2e-") or any(Path(request[k]) != v for k, v in expected.items()):
        raise ValueError(f"lane {ordinal} paths escape their fixed root")
    observed = {"source_disk_sha256": digest(expected["source_disk_path"]), "source_vars_sha256": digest(expected["source_vars_path"]),
        "source_vtpm_tree_sha256": tree_digest(expected["source_vtpm_path"]), "final_disk_sha256": digest(expected["disk_path"]),
        "final_vars_sha256": digest(expected["vars_path"]), "final_vtpm_tree_sha256": tree_digest(expected["vtpm_state_path"]),
        "guest_evidence_sha256": digest(expected["guest_evidence_path"])}
    if any(result[k] != v for k, v in observed.items()): raise ValueError(f"lane {ordinal} artifacts differ")
    if result["source_disk_sha256"] != result["imported_initial_disk_sha256"] or result["source_vars_sha256"] != result["imported_initial_vars_sha256"] or result["source_vtpm_tree_sha256"] != result["imported_initial_vtpm_tree_sha256"]:
        raise ValueError(f"lane {ordinal} imported media differs from source")
    seal = {"schema_version": "bridgevm.windows-hvf-import-product-e2e-host-stamp.v1", "job_id": job,
        "commit": commit, "lane": ordinal, "nonce": result["nonce"], "request_sha256": digest(request_path),
        "result_sha256": digest(result_path)}
    with stamp.open("x", encoding="utf-8") as output: json.dump(seal, output, indent=2, sort_keys=True); output.write("\n")

def host(command, fallback):
    try: value = subprocess.check_output(command, text=True, stderr=subprocess.DEVNULL).strip()
    except (OSError, subprocess.CalledProcessError): value = ""
    return value or fallback

def build(args):
    expected = 3 if args.mode == "release" else 1
    try: verified = load(args.verified)
    except (OSError, ValueError, json.JSONDecodeError): verified = {}
    assets = verified.get("assets", {}) if isinstance(verified, dict) else {}
    def asset(key):
        value = assets.get(key, {}).get("sha256", "absent") if isinstance(assets.get(key, {}), dict) else "absent"
        return value if isinstance(value, str) and VERIFIER.SHA256.fullmatch(value) else "absent"
    results = []
    for ordinal in range(1, args.attempts + 1):
        try: results.append(lane(args.private/f"lane-{ordinal}-result.json", args.job_id, args.commit, args.mode, ordinal, args.private/f"lane-{ordinal}-authenticated.json"))
        except (OSError, ValueError, json.JSONDecodeError): pass
    successful = [item for item in results if item["failure_code"] == "none"]
    stages = {f"{stage}_passes": sum(item[stage] is True for item in results) for stage in LANE_STAGES}
    lane_hashes = {field: aggregate([item[field] for item in successful]) for field in LANE_HASHES}
    started = datetime.strptime(args.started_at, "%Y-%m-%dT%H:%M:%SZ").replace(tzinfo=timezone.utc)
    finished = max(started, datetime.now(timezone.utc).replace(microsecond=0)); count = args.attempts; passes = len(successful)
    hashes = {"input_manifest_sha256": digest(args.input_manifest), "app_artifact_sha256": asset("app_bundle"),
        "app_executable_sha256": asset("app_executable"), "helper_sha256": asset("product_helper"),
        "runner_sha256": asset("runner"), **lane_hashes}
    complete = args.outcome == "completed"; product = len(results) == count and count > 0
    ui = passes == count and count > 0 and all(item["ui_frontend_automated"] for item in successful)
    passed = args.valid and complete and count == expected and passes == expected and product and ui and args.cleanup and all(value != "absent" for value in hashes.values())
    receipt = {"schema_version": VERIFIER.SCHEMA, "gate_id": VERIFIER.GATE_ID, "criterion": "A9", "tier": VERIFIER.TIER,
        "job_id": args.job_id, "tested_commit": args.commit, "commit": args.commit, "hosted_ci_commit": args.commit,
        "campaign_mode": args.mode, "artifact_signing_class": args.signing_class, "clean_machine": False,
        "ui_frontend_automated": ui, "product_model_automated": product, "three_d_injection": False,
        "worker_cleanup_verified": args.cleanup, "hosted_ci_green": False, "security_ci_green": False,
        "valid": args.valid, "expected_runs": expected, "run_count": count, "passes": passes, "failures": count-passes,
        "elapsed_ms": int((finished-started).total_seconds()*1000), "failure_code": args.failure_code,
        "outcome": args.outcome, "pass": passed, "claim_eligible": False, "criterion_pass": False,
        "capability_promotion": False, "host_model": host(["sysctl", "-n", "hw.model"], platform.machine() or "unknown-host"),
        "macos_version": host(["sw_vers", "-productVersion"], platform.mac_ver()[0] or "unknown-macos"),
        "started_at": started.strftime("%Y-%m-%dT%H:%M:%SZ"), "finished_at": finished.strftime("%Y-%m-%dT%H:%M:%SZ"),
        "hosted_ci_run_id": "absent", "security_ci_run_id": "absent", **hashes, **stages}
    VERIFIER.validate(receipt, expected_commit=args.commit); return receipt

def main():
    if sys.argv[1:2] == ["--check-lane"]:
        parser = argparse.ArgumentParser(); parser.add_argument("--check-lane", type=Path, required=True)
        parser.add_argument("--request", type=Path, required=True); parser.add_argument("--stamp", type=Path, required=True)
        parser.add_argument("--job-id", required=True); parser.add_argument("--commit", required=True)
        parser.add_argument("--mode", choices=("pilot", "release"), required=True); parser.add_argument("--ordinal", type=int, required=True)
        args = parser.parse_args()
        try: authenticate(args.request, args.check_lane, args.stamp, args.job_id, args.commit, args.mode, args.ordinal)
        except (OSError, ValueError, json.JSONDecodeError) as error: print(f"invalid T19 lane: {error}", file=sys.stderr); return 1
        return 0
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("out", "private", "input-manifest", "verified"): parser.add_argument(f"--{name}", type=Path, required=True)
    parser.add_argument("--job-id", required=True); parser.add_argument("--commit", required=True)
    parser.add_argument("--mode", choices=("pilot", "release"), required=True); parser.add_argument("--attempts", type=int, default=0)
    parser.add_argument("--started-at", required=True); parser.add_argument("--outcome", choices=tuple(VERIFIER.OUTCOMES), required=True)
    parser.add_argument("--failure-code", choices=tuple(VERIFIER.FAILURE_CODES), required=True)
    parser.add_argument("--signing-class", choices=("unverified", "development-ad-hoc", "development-signed", "developer-id-notarized"), default="unverified")
    parser.add_argument("--cleanup", action="store_true"); parser.add_argument("--valid", action="store_true"); args = parser.parse_args()
    if not 0 <= args.attempts <= (3 if args.mode == "release" else 1): parser.error("attempt count differs")
    try:
        receipt = build(args); args.out.parent.mkdir(parents=True, exist_ok=True)
        with args.out.open("x", encoding="utf-8") as output: json.dump(receipt, output, indent=2, sort_keys=True); output.write("\n")
    except (OSError, ValueError, json.JSONDecodeError, VERIFIER.ReceiptError) as error:
        print(f"refusing T19 receipt: {error}", file=sys.stderr); return 1
    return 0

if __name__ == "__main__": raise SystemExit(main())
