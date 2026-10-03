#!/usr/bin/env bash
# Owned deterministic fixtures only; no Windows boot or private inputs.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
python3 tests/integration/t22-pair-provenance-contract.py
python3 tests/integration/t22-pair-admission-contract.py
python3 tests/integration/t22-pair-preparation-contract.py
python3 tests/integration/t22-pair-process-boundary-contract.py
