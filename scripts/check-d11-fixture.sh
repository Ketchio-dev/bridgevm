#!/bin/bash
# Deterministic development fixture contracts; no VM, disk image or worker.
set -euo pipefail
cd "${BASH_SOURCE[0]%/*}/.."
for test in input cleanup receipt runtime queue attempt attempt-error; do
    python3 -B "tests/integration/d11-fixture-$test-contract.py"
done
python3 -B tests/integration/d11-fixture-source-contract.py
if [[ "$(uname -s)" == Darwin ]]; then tests/integration/d11-fixture-helper-contract.sh; fi
if command -v pwsh >/dev/null; then pwsh -NoProfile -File tests/integration/d11-fixture-powershell-contract.ps1; fi
