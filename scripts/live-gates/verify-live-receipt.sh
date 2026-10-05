#!/usr/bin/env bash
set -euo pipefail
[[ $# -eq 4 ]] || exit 2
if [[ "$1" == t1-vtimer ]]; then
    exec python3 "$3/scripts/live-gates/t1_probe_receipt.py" verify "$(dirname "$2")" "$2" "$4" "$(basename "$(dirname "$2")")"
fi
exec /bin/bash "$3/scripts/live-gates/verify-live-receipt-legacy.sh" "$@"
