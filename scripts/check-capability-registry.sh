#!/usr/bin/env bash
set -euo pipefail; cd "$(dirname "${BASH_SOURCE[0]}")/.."
python3 tests/integration/capability-freshness-smoke.py
python3 scripts/render-capability-status.py --check
