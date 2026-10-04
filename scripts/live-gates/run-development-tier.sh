#!/usr/bin/env bash
set -euo pipefail
TIER="$1"; OUT="$2"; JOB_ID="$3"; REPO="$4"; INPUT="$5"; BINARY="$6"
COMMIT="$(/usr/bin/git --no-optional-locks -C "$REPO" rev-parse HEAD)"
case "$TIER" in
    d10-t22-owned-pair-preparation) exec /bin/bash --noprofile --norc -p "$REPO/scripts/live-gates/t22-pair-queue-dispatch.sh" run "$OUT" "$REPO" "$COMMIT" "$INPUT" "$BINARY" ;;
    d9-b9-real-workload) exec python3 "$REPO/scripts/live-gates/b9_real_workload_queue.py" run "$OUT" "$REPO" "$COMMIT" "$INPUT" "$BINARY" ;;
    d5-guest-input) exec python3 "$REPO/scripts/live-gates/guest_input_queue.py" run "$OUT" "$REPO" "$COMMIT" "$INPUT" "$BINARY" ;;
    d4-winpe-companions) exec python3 "$REPO/scripts/live-gates/run-winpe-companions.py" --out "$OUT" --job-id "$JOB_ID" --input-manifest "$INPUT" --sealed-binary "$BINARY" ;;
    *) exit 2 ;;
esac
