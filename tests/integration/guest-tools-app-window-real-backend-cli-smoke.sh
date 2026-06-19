#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

BRIDGEVM_APP_WINDOW_REAL_BACKEND=1 \
  bash "$ROOT/tests/integration/guest-tools-app-window-cli-smoke.sh"
