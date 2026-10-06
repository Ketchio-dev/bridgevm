#!/usr/bin/env bash
# Real image chains and release helper selection; no guest media or live runner.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
scripts/check-bundle-import-storage-cases.sh
python3 tests/integration/bundle-import-release-helper-contract.py
