"""Measured host-only payloads for copied-worker tests; never boot a VM."""
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/live-gates"))

from guest_input_live_inputs import clone_media, digest
from retained_windows_identity import directory_identity
from t22_pair_admission import screen_disk, shutdown_observed
from t22_pair_boot import boot_command
from t22_pair_launch import OwnedLaunch
from t22_pair_provenance import file_hash
from t22_pair_publication import publish, record
from t22_pair_queue_inputs import bound, stable_output
from t22_pair_fixtures import report, write_json
from t17_terminal_report_tail import BANNER, SERIAL, FOOTER


def measured_core(directory, root, commit, mode):
    identity, rows, native, documents = bound(directory, root, commit)
    output = stable_output(identity["job_id"])
    for parent in (output.parent.parent, output.parent):
        parent.mkdir(mode=0o700, exist_ok=True)
    output.mkdir(mode=0o700)
    receipt = {"schema": "bridgevm.t22-owned-pair-preparation.v1", "commit": commit,
               "job_id": identity["job_id"], "purpose": "development-only",
               "claim_eligible": False, "criterion_pass": False,
               "future_tpm_independence_proven": False,
               "input_manifest_sha256": hashlib.sha256(native).hexdigest(),
               "origin_manifest_sha256": identity["origin_manifest_sha256"],
               "query_script_sha256": identity["query_script_sha256"],
               "vtpm_configured": False, "initial_boot_writes_owned_clones": True,
               "preparation_complete": mode == "success", "complete": mode == "success",
               "cleanup_complete": True, "source_integrity": True,
               "encryption_observed_after_initial_boot": True,
               "natural_shutdown_observed": mode == "success"}
    paths = {key: Path(rows[key][0]) for key in ("image", "vars")}
    receipt.update(screen_disk(paths["image"]))
    work = output / "live"
    clones = clone_media(paths, {key: rows[key][1] for key in paths}, work)
    share = work / "share"; share.mkdir(mode=0o700)
    shutil.copyfile(root / "scripts/win-assets/bv-t22-pair-admission.ps1",
                    share / "bv-t22-pair-admission.ps1")
    (work / "agent.ctl").touch(mode=0o600)
    launch = OwnedLaunch(work, directory_identity(work), receipt)
    try:
        launch.attempt()
        if mode == "unknown-spawn":
            launch.finish(None, False)
            receipt.update(cleanup_complete=False, complete=False, preparation_complete=False,
                           natural_shutdown_observed=False, failure_type="InterruptedError")
        else:
            process = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(.1)", str(output)],
                                       start_new_session=True, stdin=subprocess.DEVNULL,
                                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            launch.started(process); process.wait(timeout=5); launch.finish(process, True)
    finally:
        launch.close()
    nonce = "1" * 32
    raw = json.dumps(report(nonce, receipt["query_script_sha256"]), sort_keys=True).encode()
    query = share / f"result-{nonce}.json"; query.write_bytes(raw)
    done = share / f"result-{nonce}.done"; done.write_bytes(hashlib.sha256(raw).hexdigest().encode())
    receipt.update(query_result_sha256=hashlib.sha256(raw).hexdigest(), fixed_volume_count=3,
                   decrypted_ntfs_volume_count=2)
    boot = work / "boot"; boot.mkdir(mode=0o700)
    stop = "PSCI 0x84000008 (system off)" if mode == "success" else "watchdog"
    log = (BANNER + ("stop: " + stop + "\n").encode()
           + b"host media: NVMe disk written back: owned.raw (67108864 bytes)\n"
           + b"serial raw bytes: 00000000 output bytes: 00000000" + SERIAL + FOOTER)
    (boot / "run.log").write_bytes(log)
    if mode == "success": receipt["shutdown_log_sha256"] = shutdown_observed(0, boot / "run.log")
    command, _, firmware = boot_command(rows, clones, work)
    receipt.update(firmware_sha256=digest(firmware),
                   boot_config_sha256=hashlib.sha256("\0".join(command).encode()).hexdigest())
    if receipt["cleanup_complete"]:
        for clone in clones.values(): clone.chmod(0o400)
        receipt["output_hashes"] = {key: file_hash(path, readonly=True) for key, path in clones.items()}
    if mode == "cleanup-false":
        receipt.update(cleanup_complete=False, complete=False, preparation_complete=False,
                       natural_shutdown_observed=False)
        for clone in clones.values(): clone.chmod(0o600)
    if mode == "marker": (output / "cleanup-required.env").write_text("cleanup_complete=false\n")
    if mode == "missing-query-done": done.unlink()
    if mode == "invalid-query":
        value = json.loads(raw); value["volumes"][1]["encryption_method"] = 6
        raw = json.dumps(value, sort_keys=True).encode(); query.write_bytes(raw)
        done.write_bytes(hashlib.sha256(raw).hexdigest().encode())
        receipt["query_result_sha256"] = hashlib.sha256(raw).hexdigest()
    if mode == "missing-owned-context": (work / "owned-launch-context.json").unlink()
    if mode in ("live-group", "unqueryable-group"):
        path = work / "owned-launch-context.json"; value = json.loads(path.read_bytes())
        value["pid"] = int((directory / "fixture-group.pid").read_text()) if mode == "live-group" else None
        receipt["owned_launch_context_sha256"] = write_json(path, value)
    if receipt["preparation_complete"]: publish(rows, clones, output, directory_identity(output), receipt, commit)
    record(output, directory_identity(output), receipt)
    return output


if __name__ == "__main__":
    directory, root, commit, mode = sys.argv[1:]
    measured_core(Path(directory), Path(root), commit, mode)
