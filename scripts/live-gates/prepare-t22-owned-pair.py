#!/usr/bin/env python3
"""Prepare development-only T22 inputs from a stopped authenticated retained pair.

The initial boot writes exclusively owned clones. Encryption status is queried
after that boot; this is neither prewrite clearance nor a release criterion.
No vTPM state, protectors or recovery keys are opened; source media stay immutable.
"""
import argparse
import json
import os
from pathlib import Path
import platform
import re
import subprocess

from retained_windows_identity import directory_identity
from t22_pair_provenance import admit, unchanged
from t22_pair_runtime import execute
from t22_pair_environment import controlled_env, operations_environment
from t22_pair_publication import publish, record


def require_source(root, commit):
    git = ["/usr/bin/git", "--no-optional-locks"]
    head = subprocess.check_output([*git, "rev-parse", "HEAD"], cwd=root, text=True, env=controlled_env()).strip()
    dirty = subprocess.check_output([*git, "status", "--porcelain", "--untracked-files=all"], cwd=root, text=True, env=controlled_env())
    if head != commit or dirty:
        raise ValueError("preparation source differs from requested source")
    return head


def _prepare(args):
    root = Path(__file__).resolve().parents[2]
    if platform.system() != "Darwin" or platform.machine() != "arm64":
        raise ValueError("physical Apple-silicon execution required")
    if not re.fullmatch(r"[0-9a-f]{40}", args.commit) or not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]{0,95}", args.job_id):
        raise ValueError("exact source and owned job identity required")
    head = require_source(root, args.commit)
    if not args.output.is_absolute() or str(args.output.resolve()) != str(args.output):
        raise ValueError("preparation output must be canonical and absolute")
    parent = args.output.parent.lstat()
    if args.output.parent.is_symlink() or parent.st_uid != os.geteuid() or parent.st_mode & 0o077:
        raise ValueError("preparation parent must be an owned private directory")
    rows, data, documents = admit(args.input_manifest, args.input_sha256, args.origin_manifest, args.origin_sha256, args.commit)
    args.output.mkdir(mode=0o700, exist_ok=False)
    identity = directory_identity(args.output)
    receipt = {"schema": "bridgevm.t22-owned-pair-preparation.v1", "commit": head,
               "job_id": args.job_id, "purpose": "development-only", "claim_eligible": False,
               "criterion_pass": False, "future_tpm_independence_proven": False,
               "input_manifest_sha256": args.input_sha256, "origin_manifest_sha256": args.origin_sha256,
               "preparation_complete": False, "encryption_observed_after_initial_boot": False,
               "natural_shutdown_observed": False, "source_integrity": False}
    try:
        clones = execute(rows, args.input_manifest, data, documents, args.output, receipt)
        if clones:
            require_source(root, args.commit)
            unchanged(args.input_manifest, data, documents, rows)
            publish(rows, clones, args.output, identity, receipt, args.commit)
    except BaseException as error:
        receipt.update({"preparation_complete": False, "complete": False, "source_integrity": False, "failure_type": type(error).__name__})
    record(args.output, identity, receipt)
    print(json.dumps(receipt, sort_keys=True))
    return 0 if receipt["preparation_complete"] and "t22_input_manifest_sha256" in receipt else 1


def prepare(args):
    with operations_environment():
        return _prepare(args)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for key in ("commit", "job-id", "input-sha256", "origin-sha256"):
        parser.add_argument("--" + key, required=True)
    for key in ("input-manifest", "origin-manifest", "output"):
        parser.add_argument("--" + key, required=True, type=Path)
    try:
        return prepare(parser.parse_args())
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        print(json.dumps({"claim_eligible": False, "preparation_complete": False, "cleanup_required": True, "failure_type": type(error).__name__}))
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
