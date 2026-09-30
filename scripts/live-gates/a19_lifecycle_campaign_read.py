#!/usr/bin/env python3
"""Strict T23 campaign read: queue seal, retained lane records, residue and CLI.

verify    check a private, staged or public receipt against its sealed job
missing   write the nonpassing receipt the worker records for a lost runner
fence     succeed only when the sealed receipt proves every lane was cleaned
"""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
from pathlib import Path
import re
import sys

from a19_lifecycle_campaign_receipt import LANE_FIELDS, TIER, initial, lane_lists, validate, write_new
from a19_lifecycle_campaign_record import IDENTITY, lanes_clean, read_records
from native_snapshot_restore_public import load_receipt
from native_snapshot_restore_receipt import TIER as T20_TIER
from native_snapshot_restore_receipt_path import verified_receipt as t20_verified_receipt
from native_snapshot_restore_seal import COMMIT, JOB_ID, SHA256, _env, validate_seal as t20_validate_seal

LEDGER = ("job_id", "tier", "commit", "input_manifest_sha256", "sealed_binary_sha256")
STAGED_PUBLIC = re.compile(r"\.receipt\.public\.[0-9]+\.json\Z")


def sealed_hashes(job_dir: Path, job_id: str, commit: str) -> dict[str, str]:
    if not JOB_ID.fullmatch(job_id) or not COMMIT.fullmatch(commit):
        raise ValueError("T23 queue identity is invalid")
    if job_dir.name != job_id or job_dir.is_symlink() or not job_dir.is_dir():
        raise ValueError("T23 job directory differs from its sealed identity")
    ledger_root = job_dir.parent.parent / "job-ledger"
    ledger_dir = ledger_root / job_id
    if ledger_root.is_symlink() or ledger_dir.is_symlink() or not ledger_dir.is_dir():
        raise ValueError("T23 ledger directory is unsafe")
    job = _env(job_dir / "job.env")
    ledger = _env(ledger_dir / "entry.env", readonly=True)
    if not set(LEDGER).issubset(job) or set(ledger) != set(LEDGER):
        raise ValueError("T23 queue seal has missing or unexpected fields")
    for rows in (job, ledger):
        for field, expected in (("job_id", job_id), ("tier", TIER), ("commit", commit)):
            if rows[field] != expected:
                raise ValueError(f"T23 queue seal differs from {field}")
    for field in ("input_manifest_sha256", "sealed_binary_sha256"):
        if not SHA256.fullmatch(job[field]) or ledger[field] != job[field]:
            raise ValueError(f"T23 queue seal differs from {field}")
    return {"input_manifest_sha256": job["input_manifest_sha256"],
            "binary_hash": job["sealed_binary_sha256"]}


def validate_seal(value: dict, job_dir: Path) -> None:
    """The receipt matches its queue seal, clean lanes and every retained lane record."""
    if value["worker_cleanup_verified"] is not True:
        raise ValueError("T23 receipt cannot publish before every lane's private media is removed")
    for field, expected in sealed_hashes(job_dir, value["job_id"], value["commit"]).items():
        if value[field] != expected:
            raise ValueError(f"T23 receipt differs from sealed {field}")
    if not lanes_clean(job_dir):
        raise ValueError("T23 receipt cannot publish with owned private media present")
    identity = {field: value[field] for field in IDENTITY}
    retained = lane_lists(read_records(job_dir / "lanes", identity, value["run_count"]))
    if retained != {field: value[field] for field in LANE_FIELDS}:
        raise ValueError("T23 receipt lane evidence differs from its retained lane records")


def verified_receipt(path: Path, job_dir: Path, expected_commit: str | None) -> dict:
    if path.parent.resolve() != job_dir.resolve():
        raise ValueError("T23 receipt is outside its sealed job directory")
    name = path.name
    if name not in ("receipt.json", "receipt.public.json") and not STAGED_PUBLIC.fullmatch(name):
        raise ValueError("T23 receipt has an unexpected filename")
    value = validate(load_receipt(path), expected_commit)
    if name != "receipt.json" and value != validate(load_receipt(job_dir / "receipt.json"), expected_commit):
        raise ValueError("T23 public receipt differs from its private original")
    return value


def read_strict(public: Path, directory: Path) -> dict:
    value = verified_receipt(public, directory, None)
    validate_seal(value, directory)
    return value


def _read_t20(public: Path, directory: Path) -> dict:
    value = t20_verified_receipt(public, directory, None)
    t20_validate_seal(value, directory)
    return value


READERS = {T20_TIER: _read_t20, TIER: read_strict}


def strict_reader(tiers: tuple[object, ...]):
    """The strict reader when job, ledger or public receipt names a strict tier."""
    for tier, reader in READERS.items():
        if tier in tiers:
            return reader
    return None


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("mode", choices=("verify", "missing", "fence"))
    parser.add_argument("path", type=Path)
    parser.add_argument("--expected-commit")
    parser.add_argument("--job-id")
    parser.add_argument("--job-dir", type=Path)
    args = parser.parse_args()
    try:
        if args.mode == "verify":
            if args.job_dir is None:
                parser.error("verify requires --job-dir")
            validate_seal(verified_receipt(args.path, args.job_dir, args.expected_commit), args.job_dir)
            return 0
        if not args.job_id or not args.expected_commit:
            parser.error(f"{args.mode} requires --job-id and --expected-commit")
        if args.mode == "fence":
            value = verified_receipt(args.path / "receipt.json", args.path, args.expected_commit)
            validate_seal(value, args.path)
            return 0 if value["job_id"] == args.job_id else 1
        value = initial(args.job_id, args.expected_commit)
        value["finished_at"] = datetime.now(timezone.utc).isoformat()
        write_new(args.path, value)
        return 0
    except (OSError, UnicodeError, ValueError, KeyError, RecursionError) as error:
        print(f"FAIL: T23 campaign receipt {args.mode}: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
