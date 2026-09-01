#!/usr/bin/env python3
"""Audit sealed HVF boot-performance campaigns and report exploratory statistics."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

from hvf_boot_performance_report import EvidenceError, analyze, self_test


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--queue-root",
        type=Path,
        default=Path.home() / "BridgeVM" / "live-queue",
        help="physical-Mac live queue root",
    )
    parser.add_argument("--aa-campaign", help="sealed A/A campaign id")
    parser.add_argument("--ab-campaign", help="optional sealed A/B campaign id")
    parser.add_argument("--output", type=Path, help="write the successful JSON report to this path")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return 0
    if not args.aa_campaign:
        parser.error("--aa-campaign is required")
    try:
        result = analyze(args.queue_root, args.aa_campaign, args.ab_campaign)
    except EvidenceError as exc:
        print(json.dumps(exc.document(), indent=2, sort_keys=True), file=sys.stderr)
        return 1
    rendered = json.dumps(result, indent=2, sort_keys=True) + "\n"
    if args.output:
        args.output.write_text(rendered, encoding="utf-8")
    else:
        sys.stdout.write(rendered)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
