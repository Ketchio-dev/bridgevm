#!/usr/bin/env bash
set -euo pipefail
python3 tests/integration/retained-windows-publication-contract.py
python3 tests/integration/retained-windows-cleanup-races-contract.py
python3 tests/integration/retained-windows-permissions-contract.py
