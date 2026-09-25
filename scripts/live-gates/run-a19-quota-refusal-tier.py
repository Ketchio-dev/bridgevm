#!/usr/bin/env python3
"""Measure one sealed, real-media byte-quota refusal without booting a guest."""
from __future__ import annotations

from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys

from a19_quota_refusal_receipt import initial, quota_error, write_new
from native_snapshot_restore_artifacts import RELATIONS, digest, regular
from native_snapshot_restore_inputs import prepare, reauthenticate

MAX_U64 = (1 << 64) - 1
OWNED = ("prepared-inputs", "quota-refusal.snapshot", ".quota-refusal.snapshot.staging",
         "quota-boundary.snapshot", ".quota-boundary.snapshot.staging",
         ".bridgevm-snapshot-parent-lease")


def present(path: Path) -> bool:
    return os.path.lexists(path)


def invoke(helper: Path, *args: str, timeout: int) -> subprocess.CompletedProcess[bytes]:
    return subprocess.run(
        [str(helper), *args], stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        cwd=helper.parent, env={"PATH": "/usr/bin:/bin:/usr/sbin:/sbin", "LANG": "C"},
        timeout=timeout, check=False,
    )


def exercise(output: Path, prepared: Path, private: dict, receipt: dict) -> None:
    disk, vars = prepared / "disk.raw", prepared / "vars.fd"
    for path in (disk, vars):
        regular(path)
    if disk.stat().st_dev != output.stat().st_dev or vars.stat().st_dev != output.stat().st_dev:
        raise ValueError("prepared media and destination must share one volume")
    pair = disk.stat().st_size + vars.stat().st_size
    if not 1 <= pair <= MAX_U64:
        raise ValueError("invalid disk plus vars byte count")
    if shutil.disk_usage(output).free <= pair + max(64 * 1024**2, pair // 10):
        raise ValueError("insufficient space for a full-copy fallback")
    receipt.update({"pair_bytes": pair, "rejected_quota_bytes": pair - 1,
                    "accepted_quota_bytes": pair,
                    "prepared_image_sha256": digest(disk), "prepared_vars_sha256": digest(vars)})
    helper = Path(private["sealed_app"]) / RELATIONS["snapshot_helper"]
    regular(helper)
    refused = output / "quota-refusal.snapshot"
    accepted = output / "quota-boundary.snapshot"
    if any(present(output / name) for name in OWNED if name != "prepared-inputs"):
        raise ValueError("quota destination is not fresh")

    denied = invoke(helper, "create", str(disk), str(vars), str(refused),
                    "a19-quota-real-media", str(pair - 1), timeout=60)
    receipt["refusal_exit_code"] = denied.returncode
    receipt["refusal_output_sha256"] = hashlib.sha256(denied.stderr).hexdigest()
    if denied.returncode != 1 or denied.stdout or denied.stderr != quota_error(pair):
        raise ValueError("packaged helper did not report exact quota refusal")
    receipt["refusal_reason"] = "quota_exceeded"
    receipt["refusal_destination_absent"] = not any(present(output / name) for name in
                                                        ("quota-refusal.snapshot", ".quota-refusal.snapshot.staging",
                                                         ".bridgevm-snapshot-parent-lease"))
    if not receipt["refusal_destination_absent"]:
        raise ValueError("quota refusal left a destination, staging tree or lease key")
    if (digest(disk), digest(vars)) != (receipt["image_sha256"], receipt["vars_sha256"]):
        raise ValueError("quota refusal changed the selected disk or vars")

    created = invoke(helper, "create", str(disk), str(vars), str(accepted),
                     "a19-quota-real-media", str(pair), timeout=1800)
    if created.returncode != 0 or created.stderr or not created.stdout:
        raise ValueError("exact-boundary snapshot create failed")
    verified = invoke(helper, "verify", str(accepted), timeout=1800)
    if verified.returncode != 0 or verified.stderr or not verified.stdout:
        raise ValueError("exact-boundary snapshot verify failed")
    if not accepted.is_dir() or accepted.is_symlink() or {p.name for p in accepted.iterdir()} != {"disk.raw", "vars.fd", "manifest.json"}:
        raise ValueError("exact-boundary snapshot file set is invalid")
    for name in ("disk.raw", "vars.fd", "manifest.json"):
        regular(accepted / name)
    manifest = json.loads((accepted / "manifest.json").read_text(encoding="utf-8"))
    if manifest != {"format_version": 1, "vm_id": "a19-quota-real-media",
                    "disk_bytes": disk.stat().st_size, "disk_sha256": receipt["image_sha256"],
                    "vars_bytes": vars.stat().st_size, "vars_sha256": receipt["vars_sha256"]}:
        raise ValueError("exact-boundary snapshot manifest differs from sealed pair")
    receipt.update({
        "snapshot_manifest_sha256": digest(accepted / "manifest.json"),
        "snapshot_disk_sha256": digest(accepted / "disk.raw"),
        "snapshot_vars_sha256": digest(accepted / "vars.fd"),
        "final_disk_sha256": digest(disk), "final_vars_sha256": digest(vars),
    })
    if (receipt["snapshot_disk_sha256"], receipt["snapshot_vars_sha256"],
        receipt["final_disk_sha256"], receipt["final_vars_sha256"]) != (
            receipt["image_sha256"], receipt["vars_sha256"],
            receipt["image_sha256"], receipt["vars_sha256"]):
        raise ValueError("exact-boundary snapshot or selected pair changed")
    receipt["success_verified"] = True


def clean_owned(output: Path) -> bool:
    for name in OWNED:
        path = output / name
        if path.is_symlink():
            return False
        if path.is_dir():
            shutil.rmtree(path)
        elif present(path):
            return False
    return not any(present(output / name) for name in OWNED)


def main() -> int:
    if len(sys.argv) != 5:
        print("usage: run-a19-quota-refusal-tier.py OUT JOB_ID MANIFEST SEALED_BINARY", file=sys.stderr)
        return 2
    output, manifest, sealed_binary = map(Path, (sys.argv[1], sys.argv[3], sys.argv[4]))
    repo = Path(__file__).resolve().parents[2]
    commit = subprocess.check_output(["git", "-C", str(repo), "rev-parse", "HEAD"], text=True).strip()
    receipt = initial(sys.argv[2], commit)
    receipt["host_model"] = subprocess.check_output(["sysctl", "-n", "hw.model"], text=True).strip()
    receipt["macos_version"] = platform.mac_ver()[0]
    status = 1
    try:
        prepared = output / "prepared-inputs"
        public, private = prepare(manifest, sealed_binary, commit, prepared)
        receipt.update(public)
        receipt["outcome"] = "failed"
        exercise(output, prepared, private, receipt)
        reauthenticate(private, sealed_binary)
        status = 0
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError, json.JSONDecodeError) as error:
        print(f"FAIL: A19 quota tier: {type(error).__name__}", file=sys.stderr)
    finally:
        try:
            receipt["worker_cleanup_verified"] = clean_owned(output)
        except OSError:
            receipt["worker_cleanup_verified"] = False
        if status == 0 and receipt["worker_cleanup_verified"]:
            receipt.update({"outcome": "completed", "pass": True,
                            "run_count": 1, "quota_case_count": 1})
        else:
            status = 1
        receipt["finished_at"] = datetime.now(timezone.utc).isoformat()
        write_new(output / "receipt.json", receipt)
    return status


if __name__ == "__main__":
    raise SystemExit(main())
