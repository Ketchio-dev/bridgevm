"""Synthetic A19 T23 campaign fixtures: queue seals, lane records and receipts."""
from __future__ import annotations

import atexit, hashlib, json, os, shutil, subprocess, sys, tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/live-gates"))
import a19_lifecycle_campaign_receipt as receipt  # noqa: E402
import a19_lifecycle_campaign_record as record  # noqa: E402

COMMIT = subprocess.check_output(["git", "-C", str(ROOT), "rev-parse", "HEAD"], text=True).strip()
MANIFEST, BINARY, IMAGE, VARS = "b" * 64, "c" * 64, "d" * 64, "e" * 64
TIER = receipt.TIER
# Hosted macOS runners report hw.model VirtualMac2,1, which receipts refuse; fixture runs see a physical model.
FAKE_BIN = Path(tempfile.mkdtemp(prefix="t23-fake-sysctl-")); atexit.register(shutil.rmtree, FAKE_BIN, True)
(FAKE_BIN / "sysctl").write_text('#!/bin/sh\n[ "$*" = "-n hw.model" ] && { echo Mac17,9; exit 0; }\nexec /usr/sbin/sysctl "$@"\n'); (FAKE_BIN / "sysctl").chmod(0o755)


def sha(label: str) -> str:
    return hashlib.sha256(label.encode()).hexdigest()


def identity(job_id: str, commit: str = COMMIT, manifest: str = MANIFEST, binary: str = BINARY) -> dict:
    return {"job_id": job_id, "commit": commit, "input_manifest_sha256": manifest, "binary_hash": binary}


def inputs(manifest: str = MANIFEST, binary: str = BINARY) -> dict:
    return {"input_manifest_sha256": manifest, "binary_hash": binary, "image_sha256": IMAGE,
            "vars_sha256": VARS, "app_artifact_sha256": sha("app"), "app_cli_sha256": sha("cli"),
            "app_executable_sha256": sha("exe"), "snapshot_helper_sha256": sha("helper")}


def lane(ident: dict, ordinal: int, passing: bool = True) -> dict:
    value = record.new_record(ident, ordinal)
    value.update({field: sha(f"{ident['job_id']}-{ordinal}-{field}") for field in record.HASHES})
    value.update(prepared_image_sha256=IMAGE, prepared_vars_sha256=VARS,
                 restored_marker_sha256=value["original_marker_sha256"], boots_attempted=3,
                 boots_passed=3, natural_shutdown_count=3, cleanup_verified=True)
    value["pass"] = passing
    if not passing:
        value.update(boots_passed=2, natural_shutdown_count=2)
    return value


def queue_job(root: Path, job_id: str = "t23-fixture", commit: str = COMMIT, state: str = "done",
              manifest: str = MANIFEST, binary: str = BINARY) -> Path:
    job = root / "queue" / state / job_id
    ledger = root / "queue/job-ledger" / job_id
    job.mkdir(parents=True)
    ledger.mkdir(parents=True)
    rows = {"job_id": job_id, "tier": TIER, "commit": commit,
            "input_manifest_sha256": manifest, "sealed_binary_sha256": binary}
    content = "".join(f"{key}={value}\n" for key, value in rows.items())
    (job / "job.env").write_text(content + "submitted_at=synthetic\n", encoding="utf-8")
    (ledger / "entry.env").write_text(content, encoding="utf-8")
    (ledger / "entry.env").chmod(0o400)
    return job


def write_lanes(job: Path, ident: dict, count: int, failing: int | None = None) -> None:
    lanes = job / "lanes"
    lanes.mkdir(mode=0o700, exist_ok=True)
    for ordinal in range(1, count + 1):
        directory = lanes / record.lane_name(ordinal)
        directory.mkdir(mode=0o700)
        record.write_record(directory / record.RECORD, lane(ident, ordinal, ordinal != failing), ident, ordinal)


def campaign(job: Path, count: int = 10, failing: int | None = None, outcome: str | None = None,
             commit: str = COMMIT) -> dict:
    """Retain lane records in job and return the receipt the runner would write."""
    ident = identity(job.name, commit)
    write_lanes(job, ident, count, failing)
    value = receipt.initial(job.name, commit)
    value.update(inputs(), host_model="Mac17,9", macos_version="27.0",
                 started_at="2026-09-30T00:00:00+00:00", finished_at="2026-09-30T01:20:00+00:00")
    if outcome is None:
        outcome = "completed" if count == 10 and failing is None else "failed"
    value.update(outcome=outcome, failure_code={"completed": "none", "failed": "lane-failed",
                                                "canceled": "canceled"}[outcome])
    receipt.aggregate(value, record.read_records(job / "lanes", ident, count))
    value["worker_cleanup_verified"] = record.lanes_clean(job)
    value["pass"] = receipt.computed_pass(value)
    return receipt.validate(value, commit)


def write_receipt(job: Path, value: dict, name: str = "receipt.json") -> Path:
    path = job / name
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    return path


def run(command: list[str], **kwargs) -> subprocess.CompletedProcess:
    env = kwargs.pop("env", None) or dict(os.environ); env = {**env, "PATH": f"{FAKE_BIN}{os.pathsep}{env.get('PATH', os.defpath)}"}
    return subprocess.run(command, capture_output=True, text=True, timeout=60, env=env, **kwargs)


def verify(path: Path, commit: str = COMMIT) -> subprocess.CompletedProcess:
    return run(["bash", str(ROOT / "scripts/live-gates/verify-live-receipt.sh"), TIER, str(path), str(ROOT), commit])


def publish(job: Path, commit: str = COMMIT) -> subprocess.CompletedProcess:
    return run(["bash", str(ROOT / "scripts/live-gates/publish-receipt.sh"), TIER, str(job), str(ROOT), commit])


def fence(job: Path, commit: str = COMMIT, tier: str = TIER) -> subprocess.CompletedProcess:
    return run(["bash", "-c", 'source "$1"; bridgevm_t17_guard_or_fence "$2" "$3" "$4" "$5" "$6" "$7"', "_",
                str(ROOT / "scripts/live-gates/t17-worker-cleanup-fence.sh"), tier, str(job), str(ROOT),
                commit, job.name, str(job.parent.parent)])


def live_receipt(root: Path, job_id: str) -> subprocess.CompletedProcess:
    return subprocess.run(["bash", str(ROOT / "scripts/live-gates/bridgevm-live"), "receipt", job_id],
                          capture_output=True, timeout=60,
                          env=dict(os.environ, BRIDGEVM_LIVE_ROOT=str(root / "queue")))
