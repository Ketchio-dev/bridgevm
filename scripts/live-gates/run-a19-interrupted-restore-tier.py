#!/usr/bin/env python3
"""One sealed, nonpromoting interrupted restore on private real-media clones."""
from __future__ import annotations

from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import sys

from a19_interrupted_restore_receipt import initial, write_new
from a19_interrupted_restore_seal import merge_prepared, sealed_hashes
from native_snapshot_restore_artifacts import RELATIONS, digest, regular
from native_snapshot_restore_inputs import prepare, reauthenticate
from native_snapshot_export_evidence import load_evidence

SHA256 = re.compile(r"[0-9a-f]{64}\Z")
VM_ID = "a19-native-cli-live"
PHASES = ("phase1-original", "phase3-clobber", "phase5-postkill", "phase7-restored")


def read_hash(path: Path) -> str:
    regular(path)
    value = path.read_text(encoding="utf-8").strip()
    if not SHA256.fullmatch(value):
        raise ValueError("retained hash is invalid")
    return value


def file_hash(path: Path) -> str:
    return digest(path)


def parse_stop(path: Path, raw: Path) -> dict:
    regular(path)
    regular(raw)
    value = json.loads(path.read_text(encoding="utf-8"))
    expected = {"interruption_stage", "helper_stop_verified", "staged_file_sync_order_verified",
                "old_selection_before_kill", "helper_killed_and_reaped", "stop_fd_log_sha256"}
    if not isinstance(value, dict) or set(value) != expected:
        raise ValueError("stop observation field set is invalid")
    if value["interruption_stage"] != "staged-disk-verify-read" or any(
        value[field] is not True for field in expected - {"interruption_stage", "stop_fd_log_sha256"}
    ):
        raise ValueError("stop observation does not prove the declared point")
    if value["stop_fd_log_sha256"] != file_hash(raw):
        raise ValueError("retained FD observation changed")
    return value


def export_fields(output: Path) -> dict:
    return load_evidence(output / "export-evidence.json", VM_ID)


def snapshot_hashes(path: Path) -> tuple[str, str]:
    regular(path)
    value = json.loads(path.read_text(encoding="utf-8"))
    if (not isinstance(value, dict) or set(value) != {"format_version", "vm_id", "disk_bytes",
        "disk_sha256", "vars_bytes", "vars_sha256"} or value["format_version"] != 1 or
        value["vm_id"] != VM_ID or any(type(value[key]) is not int or value[key] <= 0
        for key in ("disk_bytes", "vars_bytes")) or any(not isinstance(value[key], str) or
        not SHA256.fullmatch(value[key]) for key in ("disk_sha256", "vars_sha256"))):
        raise ValueError("created snapshot manifest is invalid")
    return value["disk_sha256"], value["vars_sha256"]


def shutdown_count(output: Path) -> int:
    count = 0
    for phase in PHASES:
        log = output / phase / "run.log"
        if log.is_file() and "stop: PSCI " in log.read_text(encoding="utf-8", errors="replace"):
            count += 1
    return count


def run_lifecycle(repo: Path, environment: dict) -> int:
    child = subprocess.Popen(
        [str(repo / "scripts/verify-native-snapshot-interrupted-restore.sh")],
        cwd=repo, env=environment,
    )
    try:
        return child.wait(timeout=1800)
    except subprocess.TimeoutExpired:
        child.terminate()
        try:
            child.wait(timeout=30)
        except subprocess.TimeoutExpired:
            child.kill()
            child.wait(timeout=5)
        return 124


def collect(output: Path, receipt: dict) -> None:
    stop = parse_stop(output / "interrupt-observation.json",
                      output / "interrupt-helper-fd.private.log")
    receipt.update(stop)
    postkill = export_fields(output / "postkill-export")
    postretry = export_fields(output / "postretry-export")
    snapshot_disk, snapshot_vars = snapshot_hashes(output / "snapshot-created-manifest.json")
    receipt.update({
        "snapshot_create_result_sha256": file_hash(output / "create.json"),
        "snapshot_restore_retry_result_sha256": file_hash(output / "restore-retry.json"),
        "snapshot_disk_sha256": snapshot_disk, "snapshot_vars_sha256": snapshot_vars,
        "preinterrupt_disk_sha256": read_hash(output / "pre-interrupt-disk.sha256"),
        "preinterrupt_vars_sha256": read_hash(output / "pre-interrupt-vars.sha256"),
        "postkill_disk_sha256": read_hash(output / "postkill-disk.sha256"),
        "postkill_vars_sha256": read_hash(output / "postkill-vars.sha256"),
        "postretry_disk_sha256": postretry["disk_sha256"],
        "postretry_vars_sha256": postretry["vars_sha256"],
        "postkill_export_result_sha256": postkill["result_sha256"],
        "postkill_export_manifest_sha256": postkill["manifest_sha256"],
        "postretry_export_result_sha256": postretry["result_sha256"],
        "postretry_export_manifest_sha256": postretry["manifest_sha256"],
        "original_marker_sha256": file_hash(output / "phase1-original/marker-after.txt"),
        "clobber_marker_sha256": file_hash(output / "phase3-clobber/marker-after.txt"),
        "postkill_marker_sha256": file_hash(output / "phase5-postkill/marker-before.txt"),
        "restored_marker_sha256": file_hash(output / "phase7-restored/marker-before.txt"),
    })
    if (postkill["disk_sha256"], postkill["vars_sha256"]) != (
        receipt["postkill_disk_sha256"], receipt["postkill_vars_sha256"]):
        raise ValueError("postkill export differs from independent hash")
    receipt["postkill_pair_unchanged"] = (
        receipt["preinterrupt_disk_sha256"], receipt["preinterrupt_vars_sha256"]
    ) == (receipt["postkill_disk_sha256"], receipt["postkill_vars_sha256"])
    receipt["postretry_original_restored"] = (
        receipt["postretry_disk_sha256"], receipt["postretry_vars_sha256"],
        receipt["restored_marker_sha256"],
    ) == (snapshot_disk, snapshot_vars, receipt["original_marker_sha256"])
    if not (receipt["postkill_pair_unchanged"] and receipt["postretry_original_restored"] and
            receipt["postkill_marker_sha256"] == receipt["clobber_marker_sha256"] and
            receipt["original_marker_sha256"] != receipt["clobber_marker_sha256"]):
        raise ValueError("interrupted or retried guest pair identity is wrong")


def main() -> int:
    if len(sys.argv) != 5:
        print("usage: run-a19-interrupted-restore-tier.py OUT JOB_ID MANIFEST SEALED_BINARY", file=sys.stderr)
        return 2
    output, manifest, sealed_binary = map(Path, (sys.argv[1], sys.argv[3], sys.argv[4]))
    repo = Path(__file__).resolve().parents[2]
    commit = subprocess.check_output(["git", "-C", str(repo), "rev-parse", "HEAD"], text=True).strip()
    receipt = initial(sys.argv[2], commit)
    receipt["host_model"] = subprocess.check_output(["sysctl", "-n", "hw.model"], text=True).strip()
    receipt["macos_version"] = platform.mac_ver()[0]
    status = 1
    prepared = output / "prepared-inputs"
    try:
        receipt.update(sealed_hashes(output, sys.argv[2], commit))
        public, private = prepare(manifest, sealed_binary, commit, prepared)
        merge_prepared(receipt, public)
        receipt["prepared_image_sha256"] = digest(prepared / "disk.raw")
        receipt["prepared_vars_sha256"] = digest(prepared / "vars.fd")
        pair = (prepared / "disk.raw").stat().st_size + (prepared / "vars.fd").stat().st_size
        if shutil.disk_usage(output).free <= pair + max(64 * 1024**2, pair // 10):
            raise ValueError("insufficient private APFS space")
        helper = Path(private["sealed_app"]) / RELATIONS["snapshot_helper"]
        regular(helper)
        environment = dict(
            os.environ, OUT=str(output), DISK=str(prepared / "disk.raw"),
            VARS=str(prepared / "vars.fd"), BRIDGEVM_PREBUILT_PROBE=private["binary"],
            NATIVE_SNAPSHOT_CLI=private["app_cli"], A19_SNAPSHOT_HELPER=str(helper),
            NATIVE_SNAPSHOT_VM_ID=VM_ID,
        )
        receipt["outcome"] = "failed"
        completed = run_lifecycle(repo, environment)
        receipt["boots_attempted"] = sum((output / phase).is_dir() for phase in PHASES)
        receipt["natural_shutdown_count"] = shutdown_count(output)
        receipt["boots_passed"] = receipt["natural_shutdown_count"]
        reauthenticate(private, sealed_binary)
        if completed != 0:
            raise RuntimeError(f"T22 lifecycle exited {completed}")
        collect(output, receipt)
        receipt["final_prepared_image_sha256"] = digest(prepared / "disk.raw")
        receipt["final_prepared_vars_sha256"] = digest(prepared / "vars.fd")
        if receipt["boots_passed"] != 4 or (output / "live").exists():
            raise ValueError("four natural shutdowns or live library cleanup missing")
        status = 0
    except (OSError, UnicodeError, ValueError, RuntimeError, subprocess.SubprocessError,
            json.JSONDecodeError) as error:
        print(f"FAIL: T22 interrupted restore: {type(error).__name__}: {error}", file=sys.stderr)
    finally:
        try:
            if prepared.is_dir() and not prepared.is_symlink():
                shutil.rmtree(prepared)
            receipt["worker_cleanup_verified"] = (
                not prepared.exists() and not prepared.is_symlink() and
                not (output / "live").exists() and not (output / "live").is_symlink()
            )
        except OSError:
            receipt["worker_cleanup_verified"] = False
        if status == 0 and receipt["worker_cleanup_verified"]:
            receipt.update({"outcome": "completed", "pass": True,
                            "run_count": 1, "interruption_case_count": 1, "sample_count": 1})
        else:
            status = 1
        receipt["finished_at"] = datetime.now(timezone.utc).isoformat()
        write_new(output / "receipt.json", receipt)
    return status


if __name__ == "__main__":
    raise SystemExit(main())
