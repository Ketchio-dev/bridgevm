#!/usr/bin/env bash
# Owned qcow2 chains and native raw bundles across both transfer formats.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
python3 tests/integration/bundle-import-portability-cli-socket-contract.py
python3 tests/integration/bundle-import-raw-cli-contract.py
