"""Serve an archived D10 receipt against queue seals, without continuous-state claims."""
import hashlib
import os

from native_snapshot_restore_seal import read_bounded_regular
from t22_pair_provenance import file_hash
from t22_pair_queue_inputs import job, parse
from t22_pair_queue_proof import document
from t22_pair_queue_receipt import checked


def read(directory, job_id):
    if directory.parent.name != "done" or directory.parent.is_symlink():
        raise ValueError("development receipt is not in the done queue")
    from native_snapshot_restore_seal import _env
    commit = _env(directory / "job.env").get("commit", "")
    identity = job(directory, commit, job_id)
    data = read_bounded_regular(directory / "input-manifest.tsv", 65_536)
    rows, _, _, origin, query = parse(data, commit)
    if (hashlib.sha256(data).hexdigest() != identity["input_manifest_sha256"]
            or origin != identity["origin_manifest_sha256"] or query != identity["query_script_sha256"]
            or rows["binary"][1] != identity["sealed_binary_sha256"]
            or file_hash(directory / "hvf_gic_boot_probe") != identity["sealed_binary_sha256"]
            or file_hash(directory / "retained-origin.json", readonly=True) != origin):
        raise ValueError("archived development seals differ")
    private, _ = document(directory / "receipt.json")
    public, _ = document(directory / "receipt.public.json")
    checked(public, identity)
    if private != public or public["worker_cleanup_verified"] is not True:
        raise ValueError("archived development receipt is unproved")
    return public
