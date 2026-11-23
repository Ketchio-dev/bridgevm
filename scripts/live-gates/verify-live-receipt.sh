#!/usr/bin/env bash
set -euo pipefail
[[ $# -eq 4 ]] || exit 2
exec /bin/bash "$3/scripts/live-gates/verify-live-receipt-legacy.sh" "$@"
