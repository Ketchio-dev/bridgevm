#!/usr/bin/env bash
# Publish through one fail-closed boundary; T17 and T18 use exact schema checks.
set -euo pipefail
[[ $# -eq 4 ]] || { echo "usage: publish-receipt.sh TIER DIR WORKTREE COMMIT" >&2; exit 2; }
TIER="$1"; DIR="$2"; WORKTREE="$3"; COMMIT="$4"; source "$WORKTREE/scripts/live-gates/development-queue-receipt-dispatch.sh"; bridgevm_development_receipt_dispatch publish "$TIER" "$DIR" "$WORKTREE" "$COMMIT"
PRIVATE="$DIR/receipt.json"; PUBLIC="$DIR/receipt.public.json"
[[ -f "$PRIVATE" && ! -L "$PRIVATE" && ! -e "$PUBLIC" ]] || exit 1
[[ "$TIER" != t18-audio-teardown ]] || exec "$WORKTREE/scripts/live-gates/publish-audio-teardown-receipt.sh" "$DIR" "$WORKTREE" "$COMMIT"
"$WORKTREE/scripts/live-gates/verify-live-receipt.sh" "$TIER" "$PRIVATE" "$WORKTREE" "$COMMIT" >/dev/null
STAGE="$DIR/.receipt.public.$$.json"; trap 'rm -f "$STAGE"' EXIT
python3 "$WORKTREE/scripts/live-gates/redact-receipt.py" --in "$PRIVATE" --out "$STAGE"
"$WORKTREE/scripts/live-gates/verify-live-receipt.sh" "$TIER" "$STAGE" "$WORKTREE" "$COMMIT" >/dev/null
python3 - "$STAGE" "$PUBLIC" <<'PY'
import os, pathlib, sys
source, destination = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
os.link(source, destination)
PY
