#!/usr/bin/env python3
"""CLI compatibility wrapper for the strict hosted QMP stress contract."""
from __future__ import annotations
import argparse
import pathlib
import sys
import qmp_stress_contract as contract

def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--out", type=pathlib.Path)
    parser.add_argument("--allow-failure", action="store_true")
    parser.add_argument("--expected-repository")
    parser.add_argument("--expected-commit")
    parser.add_argument("--expected-workflow-head-sha")
    parser.add_argument("--expected-run-id")
    parser.add_argument("--expected-run-attempt")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        contract.self_test()
        return 0
    if args.out is None:
        parser.error("--out is required unless --self-test is used")
    try:
        contract.verify(contract.artifact_root(args.out), args.allow_failure, args)
    except (contract.ContractError, OSError, UnicodeError) as error:
        print(f"QMP stress receipt: FAIL ({error})", file=sys.stderr)
        return 1
    print("PASS: hosted QMP stress receipt and retained evidence")
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
