#!/usr/bin/env python3
"""Authenticate and summarize/compare CSVs without claiming the B6 gate passed."""
import argparse
import csv
import json
import sys
from b6_frame_times import NO_CLAIM, compare, summarize

parser = argparse.ArgumentParser(description=__doc__)
commands = parser.add_subparsers(dest="command", required=True)
summary = commands.add_parser("summary")
summary.add_argument("path")
summary.add_argument("sha256")
comparison = commands.add_parser("compare")
for name in ("baseline", "baseline_sha256", "candidate", "candidate_sha256"):
    comparison.add_argument(name)
args = parser.parse_args()
try:
    result = summarize(args.path, args.sha256)[0] if args.command == "summary" else compare(
        args.baseline, args.baseline_sha256, args.candidate, args.candidate_sha256)
    print(json.dumps(result, indent=2, sort_keys=True, allow_nan=False))
except (OSError, ValueError, UnicodeError, csv.Error, ArithmeticError) as error:
    print(json.dumps({"valid": False, **NO_CLAIM, "detail": str(error)}))
    sys.exit(1)
