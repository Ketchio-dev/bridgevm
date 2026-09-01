#!/usr/bin/env bash
# Publish through one fail-closed boundary; T17 is schema-checked before and after redaction.
set -euo pipefail
[[ $# -eq 4 ]] || { echo "usage: publish-receipt.sh TIER DIR WORKTREE COMMIT" >&2; exit 2; }
TIER="$1"; DIR="$2"; WORKTREE="$3"; COMMIT="$4"
PRIVATE="$DIR/receipt.json"; PUBLIC="$DIR/receipt.public.json"
[[ -f "$PRIVATE" && ! -L "$PRIVATE" && ! -e "$PUBLIC" ]] || exit 1
VERIFY="$WORKTREE/scripts/verify-windows-product-e2e-receipt.py"
if [[ "$TIER" == t17-windows-hvf-product-e2e ]]; then python3 "$VERIFY" "$PRIVATE" --expected-commit "$COMMIT" >/dev/null; fi
STAGE="$DIR/.receipt.public.$$.json"; trap 'rm -f "$STAGE"' EXIT
python3 "$WORKTREE/scripts/live-gates/redact-receipt.py" --in "$PRIVATE" --out "$STAGE"
if [[ "$TIER" == t17-windows-hvf-product-e2e ]]; then python3 "$VERIFY" "$STAGE" --expected-commit "$COMMIT" >/dev/null; fi
python3 - "$STAGE" "$PUBLIC" <<'PY'
import os, pathlib, sys
source, destination = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL
descriptor = os.open(destination, flags, 0o600)
try:
    with os.fdopen(descriptor, "wb") as output: output.write(source.read_bytes())
except BaseException:
    try: os.unlink(destination)
    except OSError: pass
    raise
PY
