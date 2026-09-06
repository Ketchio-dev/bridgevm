#!/usr/bin/env python3
"""Queue adapter for a diagnostic whose receipts can never promote a capability."""
import json
from pathlib import Path
import re
import subprocess
import sys

TIER = "d1-windows-media-comparison"
FLAGS = ("pass", "claim_eligible", "criterion_pass", "capability_promotion")


def job_fields(directory):
    fields = {}
    for line in (directory / "job.env").read_text().splitlines():
        key, value = line.split("=", 1)
        if key in fields:
            raise ValueError("duplicate job field")
        fields[key] = value
    return fields


def validate(data, job):
    if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]{0,127}", data.get("job_id", "")):
        raise ValueError("unsafe job identity")
    if job.get("tier") != TIER or data.get("tier") != TIER or any(data.get(key) is not False for key in FLAGS):
        raise ValueError("invalid diagnostic identity or promotion flags")
    for key in ("job_id", "commit", "input_manifest_sha256"):
        if data.get(key) != job[key]:
            raise ValueError("receipt identity mismatch")
    if not re.fullmatch(r"[0-9a-f]{40}", data["commit"]) or not re.fullmatch(r"[0-9a-f]{64}", data["input_manifest_sha256"]):
        raise ValueError("invalid sealed identity")
    lanes = data.get("lanes")
    if not isinstance(lanes, list) or len(lanes) > 2 or type(data.get("sample_count")) is not int or data["sample_count"] != len(lanes):
        raise ValueError("invalid diagnostic count")
    if type(data.get("passes")) is not int or data["passes"] != 0 or type(data.get("required_run_count")) is not int or data["required_run_count"] != 2:
        raise ValueError("invalid count contract")
    for expected, lane in zip(("original", "reinjected"), lanes):
        if lane.get("label") != expected or lane.get("source_integrity_verified") is not True:
            raise ValueError("invalid lane order/integrity")
        if type(lane.get("exit_code")) is not int or not -255 <= lane["exit_code"] <= 255:
            raise ValueError("invalid exit code")
        for key in ("image_sha256", "vars_sha256"):
            if not re.fullmatch(r"[0-9a-f]{64}", lane.get(key, "")):
                raise ValueError("invalid lane digest")
    if data.get("outcome") not in ("diagnostic-complete", "diagnostic-incomplete", "canceled"):
        raise ValueError("invalid outcome")
    if data["outcome"] == "diagnostic-complete" and len(lanes) != 2:
        raise ValueError("incomplete diagnostic labeled complete")


def finalize(directory, commit):
    job = job_fields(directory)
    if job["commit"] != commit or job["tier"] != TIER:
        raise ValueError("job identity mismatch")
    raw = directory / "diagnostic/receipt.json"
    if raw.is_symlink():
        raise ValueError("unsafe diagnostic receipt")
    data = json.loads(raw.read_text()) if raw.is_file() else {
        "tier": TIER, "job_id": job["job_id"], "commit": commit,
        "input_manifest_sha256": job["input_manifest_sha256"],
        **{key: False for key in FLAGS}, "passes": 0, "sample_count": 0,
        "required_run_count": 2, "outcome": "diagnostic-incomplete", "lanes": []}
    validate(data, job)
    if (directory / "cancel.requested").exists():
        data["outcome"] = "canceled"
    target = directory / "receipt.json"
    if target.is_symlink():
        raise ValueError("unsafe final receipt")
    target.write_text(json.dumps(data, indent=2) + "\n")


def publish(directory, commit):
    if (directory / "receipt.json").is_symlink():
        raise ValueError("unsafe publication source")
    job = job_fields(directory)
    data = json.loads((directory / "receipt.json").read_text())
    validate(data, job)
    if commit != job["commit"] or ((directory / "cancel.requested").exists() and data["outcome"] != "canceled"):
        raise ValueError("publication state mismatch")
    public = {key: data[key] for key in (*FLAGS, "tier", "job_id", "commit", "input_manifest_sha256", "passes", "sample_count", "required_run_count", "outcome")}
    public["known_confounders"] = ["Diagnostic only; fixed original-then-reinjected order."]
    public["known_confounders"] += [f"{lane['label']} proof exit code {lane['exit_code']}" for lane in data["lanes"]]
    with (directory / "receipt.public.json").open("x") as output:
        output.write(json.dumps(public, indent=2) + "\n")


if __name__ == "__main__":
    mode, raw_directory, raw_repo, commit = sys.argv[1:5]
    directory, repo = Path(raw_directory), Path(raw_repo)
    if mode == "run":
        job = job_fields(directory)
        subprocess.run(["python3", str(repo / "scripts/live-gates/windows-media-comparison-runner.py"),
                        "--out", str(directory / "diagnostic"), "--input-manifest", sys.argv[5],
                        "--sealed-binary", sys.argv[6], "--job-id", job["job_id"]], check=False)
        finalize(directory, commit)
        sys.exit(1)  # This diagnostic never yields a queue-level criterion pass.
    if mode == "finalize":
        finalize(directory, commit)
    elif mode == "publish":
        publish(directory, commit)
    else:
        raise ValueError("unknown queue adapter mode")
