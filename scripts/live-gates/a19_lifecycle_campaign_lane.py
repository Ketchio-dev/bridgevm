"""One isolated A19 campaign lane: its own clones, the T20 lifecycle, verified cleanup.

A lane reauthenticates the sealed manifest, APFS-clones the canonical disk and
vars into lanes/lane-NN/prepared-inputs, runs the unchanged
verify-native-snapshot-restore-boots.sh (create, clobber, restore, export to a
fresh path, three boots each ending in a host-framed natural shutdown), and removes
its clones before the next lane may start.
"""
from __future__ import annotations

import os
from pathlib import Path
import shutil
import stat
import subprocess
import sys

from a19_lifecycle_campaign_record import BOOTS, OWNED, lane_name, new_record, vm_id
from native_snapshot_export_evidence import load_evidence, receipt_fields
from native_snapshot_restore_artifacts import regular
from native_snapshot_restore_inputs import digest, parse_manifest, prepare, reauthenticate
from native_snapshot_restore_results import T20_PHASES, file_digest, line_hash, shutdown_count

ERRORS = (OSError, UnicodeError, ValueError, RuntimeError, subprocess.SubprocessError)


def volume(path: Path) -> int:
    return os.stat(path).st_dev


def preflight(manifest: Path, commit: str, output: Path) -> None:
    """Refuse before any lane when the canonical pair cannot be cloned beside the job."""
    regular(manifest)
    rows = parse_manifest(manifest.read_bytes(), commit)
    device = volume(output)
    if any(volume(Path(rows[key][0])) != device for key in ("image", "vars")):
        raise ValueError("canonical pair is not on the job volume; stage it on the internal volume first")


def lane_environment(lane: Path, ordinal: int, private: dict) -> dict[str, str]:
    prepared = lane / "prepared-inputs"
    return dict(os.environ, OUT=str(lane), DISK=str(prepared / "disk.raw"),
                VARS=str(prepared / "vars.fd"), BRIDGEVM_PREBUILT_PROBE=private["binary"],
                NATIVE_SNAPSHOT_CLI=private["app_cli"], NATIVE_SNAPSHOT_VM_ID=vm_id(ordinal))


def verify_isolation(prepared: Path, sources: tuple[Path, Path]) -> None:
    """The lane's disk and vars are two new regular files, neither shared nor linked."""
    clones = [os.stat(prepared / name, follow_symlinks=False) for name in ("disk.raw", "vars.fd")]
    originals = [os.stat(path) for path in sources]
    if any(not stat.S_ISREG(item.st_mode) or item.st_nlink != 1 for item in clones):
        raise ValueError("lane clones must be unlinked regular files")
    if len({(item.st_dev, item.st_ino) for item in clones + originals}) != 4:
        raise ValueError("lane clones share an inode with each other or the canonical pair")
    if any(item.st_dev != originals[0].st_dev for item in clones):
        raise ValueError("lane clones are not on the canonical pair's volume")


def run_lifecycle(repo: Path, environment: dict[str, str]) -> int:
    return subprocess.run([str(repo / "scripts/verify-native-snapshot-restore-boots.sh")],
                          cwd=repo, env=environment, check=False).returncode


def clean(lane: Path) -> bool:
    prepared = lane / "prepared-inputs"
    try:
        if prepared.is_dir() and not prepared.is_symlink():
            shutil.rmtree(prepared)
        return not any(os.path.lexists(lane / name) for name in OWNED)
    except OSError:
        return False


def observe(lane: Path, ordinal: int, record: dict) -> None:
    record.update(receipt_fields(load_evidence(lane / "export-evidence.json", vm_id(ordinal))))
    record.update({
        "final_disk_sha256": line_hash(lane / "final-disk.sha256"),
        "final_vars_sha256": line_hash(lane / "final-vars.sha256"),
        "snapshot_create_result_sha256": file_digest(lane / "create.json"),
        "snapshot_restore_result_sha256": file_digest(lane / "restore.json"),
        "original_marker_sha256": file_digest(lane / "phase1-original/marker-after.txt"),
        "clobber_marker_sha256": file_digest(lane / "phase3-clobber/marker-after.txt"),
        "restored_marker_sha256": file_digest(lane / "phase5-restored/marker-before.txt"),
    })
    if (record["original_marker_sha256"] != record["restored_marker_sha256"]
            or record["clobber_marker_sha256"] == record["original_marker_sha256"]):
        raise ValueError("marker clobber and restore identity was not verified")


def run_lane(lanes: Path, ordinal: int, identity: dict, manifest: Path, sealed_binary: Path,
             repo: Path) -> tuple[dict, dict]:
    """Run one lane to a record; the record's pass needs verified cleanup."""
    lane = lanes / lane_name(ordinal)
    record = new_record(identity, ordinal)
    public: dict = {}
    passed = False
    try:
        lane.mkdir(mode=0o700)
        prepared = lane / "prepared-inputs"
        public, private = prepare(manifest, sealed_binary, identity["commit"], prepared)
        if any(public[field] != identity[field] for field in ("input_manifest_sha256", "binary_hash")):
            raise ValueError("lane inputs differ from the sealed queue hashes")
        record["prepared_image_sha256"] = digest(prepared / "disk.raw")
        record["prepared_vars_sha256"] = digest(prepared / "vars.fd")
        rows = private["source_rows"]
        verify_isolation(prepared, (Path(rows["image"][0]), Path(rows["vars"][0])))
        status = run_lifecycle(repo, lane_environment(lane, ordinal, private))
        record["boots_attempted"] = sum((lane / phase).is_dir() for phase in T20_PHASES)
        record["natural_shutdown_count"] = shutdown_count(lane, T20_PHASES)
        record["boots_passed"] = record["natural_shutdown_count"]
        reauthenticate(private, sealed_binary)
        if status != 0:
            raise RuntimeError(f"marker lifecycle exited {status}")
        observe(lane, ordinal, record)
        if record["boots_passed"] != BOOTS or os.path.lexists(lane / "live"):
            raise ValueError("natural shutdown or live-library cleanup was not verified")
        passed = True
    except ERRORS as error:
        print(f"FAIL: A19 campaign lane {ordinal}: {error}", file=sys.stderr)
    finally:
        record["cleanup_verified"] = clean(lane)
        record["pass"] = passed and record["cleanup_verified"]
    return record, public
