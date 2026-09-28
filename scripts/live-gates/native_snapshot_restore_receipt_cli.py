"""Strict T20 receipt command with a required queue seal for verification."""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
from pathlib import Path

from native_snapshot_restore_receipt import initial, validate, write_new
from native_snapshot_restore_public import load_receipt
from native_snapshot_restore_seal import validate_seal


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("verify", "missing"))
    parser.add_argument("path", type=Path)
    parser.add_argument("--expected-commit")
    parser.add_argument("--job-id")
    parser.add_argument("--job-dir", type=Path)
    args = parser.parse_args()
    if args.mode == "verify":
        if args.job_dir is None:
            parser.error("verify requires --job-dir")
        value = validate(load_receipt(args.path), args.expected_commit)
        validate_seal(value, args.job_dir)
        return 0
    if not args.job_id or not args.expected_commit:
        parser.error("missing mode requires --job-id and --expected-commit")
    value = initial(args.job_id, args.expected_commit)
    value["finished_at"] = datetime.now(timezone.utc).isoformat()
    write_new(args.path, value)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
