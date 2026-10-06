#!/usr/bin/env bash
set -euo pipefail
python3 tests/integration/t17-selected-retention-contract.py
python3 tests/integration/t17-selected-retention-mutation-contract.py
python3 tests/integration/t19-selected-final-media-contract.py
for contract in product-e2e-identity t19-guest-evidence product-e2e-stamp-mutation; do python3 "tests/integration/$contract-contract.py"; done
