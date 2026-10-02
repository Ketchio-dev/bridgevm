#!/usr/bin/env bash
# Fail on the first regression, including a failure before the final suite.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

python3 tests/integration/t17-selected-retention-contract.py
python3 tests/integration/t17-selected-retention-mutation-contract.py
python3 tests/integration/native-snapshot-export-json-contract.py
python3 tests/integration/retained-windows-publication-contract.py
python3 tests/integration/retained-windows-cleanup-races-contract.py
python3 tests/integration/a19-lifecycle-campaign-progress-contract.py
python3 tests/integration/product-e2e-identity-contract.py
