#!/usr/bin/env bash
set -euo pipefail
TIER="$1"; OUT="$2"; JOB_ID="$3"; REPO="$4"; INPUT="$5"; BINARY="$6"
COMMIT="$(/usr/bin/git --no-optional-locks -C "$REPO" rev-parse HEAD)"
case "$TIER" in
    d5-guest-input) exec python3 "$REPO/scripts/live-gates/guest_input_queue.py" run "$OUT" "$REPO" "$COMMIT" "$INPUT" "$BINARY" ;;
    *) exit 2 ;;
esac
