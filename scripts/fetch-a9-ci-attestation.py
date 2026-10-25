#!/usr/bin/env python3
"""Fetch the two GitHub-hosted CI runs an A9 release claim cites and write attestations.

This is the only A9 claim code that uses the network, through `gh api`. It writes
hosted-full-ci-attestation.json and security-quality-ci-attestation.json into --out-dir
only when both runs completed successfully at --commit; otherwise it refuses and writes
neither. scripts/a9-release-claim.py verifies the files on the JSON alone.

    scripts/fetch-a9-ci-attestation.py --commit SHA --hosted-full-run-id ID \\
        --security-quality-run-id ID --out-dir DIR

For the Hosted full project check the run head_sha is the dispatched ref, so the job
log's `checked_source_sha=` line is read too and must name the same commit.
"""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import json
from pathlib import Path
import re
import subprocess
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
import a9_ci_attestation as CI  # noqa: E402

CHECK_JOB = "scripts/check-project.sh"
SOURCE_LINE = re.compile(r"^\S+ checked_source_sha=([0-9a-f]{40})\r?$", re.M)
OUTPUTS = {"hosted-full": "hosted-full-ci-attestation.json",
           "security-quality": "security-quality-ci-attestation.json"}
MAX_LOG_BYTES = 256 * 1024 * 1024


class FetchError(RuntimeError):
    """GitHub did not report a completed green run of the expected workflow at the commit."""


def gh(path: str, *, raw: bool = False) -> bytes:
    command = ["gh", "api", *(["--allow-escape-sequences"] if raw else []), path]
    try:
        result = subprocess.run(command, capture_output=True, timeout=300)
    except FileNotFoundError as error:
        raise FetchError("gh is not installed or not on PATH") from error
    except subprocess.TimeoutExpired as error:
        raise FetchError(f"gh api {path} timed out") from error
    if result.returncode != 0:
        detail = result.stderr.decode("utf-8", "replace").strip().splitlines()
        raise FetchError(f"gh api {path} failed: {detail[-1] if detail else 'no detail'}")
    if len(result.stdout) > MAX_LOG_BYTES:
        raise FetchError(f"gh api {path} returned more than {MAX_LOG_BYTES} bytes")
    return result.stdout


def gh_json(path: str) -> dict:
    try:
        value = json.loads(gh(path).decode("utf-8"))
    except (UnicodeError, json.JSONDecodeError) as error:
        raise FetchError(f"gh api {path} returned malformed JSON") from error
    if not isinstance(value, dict):
        raise FetchError(f"gh api {path} did not return a JSON object")
    return value


def checked_source(run_id: str) -> str:
    jobs = gh_json(f"repos/{CI.REPOSITORY}/actions/runs/{run_id}/jobs?per_page=100").get("jobs")
    matches = [job for job in jobs if isinstance(job, dict) and job.get("name") == CHECK_JOB] if isinstance(jobs, list) else []
    if len(matches) != 1:
        raise FetchError(f"run {run_id} does not have exactly one {CHECK_JOB} job")
    job = matches[0]
    if job.get("status") != "completed" or job.get("conclusion") != "success":
        raise FetchError(f"run {run_id} job {CHECK_JOB} did not complete successfully")
    if type(job.get("id")) is not int or job["id"] < 1:
        raise FetchError(f"run {run_id} job {CHECK_JOB} has no job id")
    log = gh(f"repos/{CI.REPOSITORY}/actions/jobs/{job['id']}/logs", raw=True).decode("utf-8", "replace")
    found = SOURCE_LINE.findall(log)
    if len(found) != 1:
        raise FetchError(f"run {run_id} {CHECK_JOB} log has {len(found)} checked_source_sha lines, not exactly one")
    return found[0]


def attestation(kind: str, run_id: str, commit: str, fetched_at: str) -> dict:
    expected = CI.KINDS[kind]
    if not CI.RUN_ID.fullmatch(run_id):
        raise FetchError(f"{kind} run id {run_id!r} is not a decimal GitHub run id")
    run = gh_json(f"repos/{CI.REPOSITORY}/actions/runs/{run_id}")
    owners = [run.get(key) for key in ("repository", "head_repository")]
    if any(not isinstance(owner, dict) or owner.get("full_name") != CI.REPOSITORY for owner in owners):
        raise FetchError(f"run {run_id} does not run {CI.REPOSITORY} code")
    if str(run.get("id")) != run_id:
        raise FetchError(f"gh returned run {run.get('id')!r} for run {run_id}")
    if (run.get("name"), run.get("path")) != (expected["workflow_name"], expected["workflow_path"]):
        raise FetchError(f"run {run_id} is {run.get('name')!r} ({run.get('path')!r}), not {expected['workflow_name']!r}")
    if run.get("status") != "completed":
        raise FetchError(f"run {run_id} is {run.get('status')!r}, not completed; retry after it finishes")
    if run.get("conclusion") != "success":
        raise FetchError(f"{kind} run {run_id} concluded {run.get('conclusion')!r}; an A9 claim needs success")
    if run.get("head_sha") != commit:
        raise FetchError(f"{kind} run {run_id} head_sha {run.get('head_sha')!r} is not {commit}")
    value = {"schema_version": CI.SCHEMA, "repository": CI.REPOSITORY,
             "workflow_name": run["name"], "workflow_path": run["path"], "run_id": run_id,
             "run_attempt": run.get("run_attempt"), "head_sha": run["head_sha"],
             "checked_source_sha": checked_source(run_id) if expected["checked_source"] else "absent",
             "event": run.get("event"), "status": run["status"], "conclusion": run["conclusion"],
             "html_url": run.get("html_url"), "fetched_at": fetched_at}
    try:
        CI.validate(value, kind=kind, commit=commit)
    except CI.AttestationError as error:
        raise FetchError(str(error)) from error
    return value


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--commit", required=True)
    parser.add_argument("--hosted-full-run-id", required=True)
    parser.add_argument("--security-quality-run-id", required=True)
    parser.add_argument("--out-dir", type=Path, required=True)
    args = parser.parse_args()
    if not CI.COMMIT.fullmatch(args.commit):
        parser.error("--commit must be a full lowercase commit")
    fetched_at = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    targets = {kind: args.out_dir / name for kind, name in OUTPUTS.items()}
    written: list[Path] = []
    try:
        existing = [str(path) for path in targets.values() if path.exists() or path.is_symlink()]
        if existing:
            raise FetchError(f"refusing to replace {', '.join(existing)}")
        values = {"hosted-full": attestation("hosted-full", args.hosted_full_run_id, args.commit, fetched_at),
                  "security-quality": attestation("security-quality", args.security_quality_run_id, args.commit, fetched_at)}
        args.out_dir.mkdir(parents=True, exist_ok=True)
        for kind, value in values.items():
            with targets[kind].open("x", encoding="utf-8") as output:
                written.append(targets[kind])
                json.dump(value, output, indent=2, sort_keys=True)
                output.write("\n")
    except (OSError, FetchError) as error:
        for path in written:
            path.unlink(missing_ok=True)
        print(f"refusing A9 CI attestation: {error}", file=sys.stderr)
        return 1
    for kind, value in values.items():
        print(f"PASS: {kind} run {value['run_id']} {value['conclusion']} at {args.commit} -> {targets[kind]}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
