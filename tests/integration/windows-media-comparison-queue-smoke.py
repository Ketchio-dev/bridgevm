#!/usr/bin/env python3
"""Sealed diagnostic submission, fallback cancellation and publication tests."""
import copy
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("queue_adapter", ROOT / "scripts/live-gates/windows-media-comparison-queue.py")
adapter = importlib.util.module_from_spec(spec)
spec.loader.exec_module(adapter)
sha = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
with tempfile.TemporaryDirectory(prefix="media queue ") as directory:
    root = Path(directory)
    env = dict(os.environ, BRIDGEVM_LIVE_ROOT=str(root / "queue"))
    cli = ROOT / "scripts/live-gates/bridgevm-live"
    missing = subprocess.run([str(cli), "submit", adapter.TIER], env=env, capture_output=True)
    assert missing.returncode != 0
    binary = root / "binary"
    binary.write_bytes(b"not a live binary")
    import hashlib
    manifest = root / "manifest.tsv"
    manifest.write_text(f"binary\t{binary}\t{hashlib.sha256(binary.read_bytes()).hexdigest()}\n")
    job_id = subprocess.check_output([str(cli), "submit", adapter.TIER, "--sha", sha,
                                     "--input-manifest", str(manifest)], env=env, text=True).strip()
    job_dir = root / "queue/queued" / job_id
    assert (job_dir / "hvf_gic_boot_probe").read_bytes() == binary.read_bytes()
    assert (job_dir / "input-manifest.tsv").read_bytes() == manifest.read_bytes()
    # Submission seals bytes; actual strict input verification occurs at execution.
    subprocess.run([str(ROOT / "scripts/live-gates/run-tier.sh"), adapter.TIER,
                    "--out", str(job_dir), "--job-id", job_id,
                    "--input-manifest", str(job_dir / "input-manifest.tsv"),
                    "--sealed-binary", str(job_dir / "hvf_gic_boot_probe")], capture_output=True, check=False)
    result = json.loads((job_dir / "receipt.json").read_text())
    assert result["outcome"] == "diagnostic-incomplete" and result["sample_count"] == 0
    job = adapter.job_fields(job_dir)
    for field in adapter.FLAGS:
        bad = copy.deepcopy(result)
        bad[field] = True
        try:
            adapter.validate(bad, job)
            raise AssertionError(f"accepted {field}")
        except ValueError:
            pass
    for field, value in (("commit", "0" * 40), ("sample_count", True), ("passes", True),
                         ("outcome", "diagnostic-complete"), ("job_id", "/Users/private")):
        bad = copy.deepcopy(result)
        bad[field] = value
        try:
            adapter.validate(bad, job)
            raise AssertionError(f"accepted {field}")
        except ValueError:
            pass
    (job_dir / "cancel.requested").touch()
    subprocess.run([str(ROOT / "scripts/live-gates/write-missing-receipt.sh"), adapter.TIER,
                    str(job_dir), str(ROOT), job_id, sha], check=True)
    assert json.loads((job_dir / "receipt.json").read_text())["outcome"] == "canceled"
    subprocess.run([str(ROOT / "scripts/live-gates/publish-receipt.sh"), adapter.TIER,
                    str(job_dir), str(ROOT), sha], check=True)
    public = json.loads((job_dir / "receipt.public.json").read_text())
    assert public["outcome"] == "canceled" and public["claim_eligible"] is False
    assert str(root) not in json.dumps(public) and "lanes" not in public
    try:
        adapter.publish(job_dir, sha)
        raise AssertionError("overwrote public receipt")
    except FileExistsError:
        pass
print("PASS: diagnostic queue sealing, strict refusal, cancellation and non-promoting publication")
