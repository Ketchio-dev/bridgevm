#!/usr/bin/env bash
# Dedicated B7 publication boundary: exact schema before and after atomic copy.
set -euo pipefail
[[ $# -eq 3 ]] || { echo "usage: publish-audio-teardown-receipt.sh DIR WORKTREE COMMIT" >&2; exit 2; }
DIR="$1"; WORKTREE="$2"; COMMIT="$3"
PRIVATE="$DIR/receipt.json"; PUBLIC="$DIR/receipt.public.json"
VERIFY="$WORKTREE/scripts/verify-audio-teardown-receipt.py"
[[ -f "$PRIVATE" && ! -L "$PRIVATE" && ! -e "$PUBLIC" ]] || exit 1
python3 "$VERIFY" "$PRIVATE" --expected-commit "$COMMIT" >/dev/null
STAGE="$DIR/.receipt.public.$$.json"; trap 'rm -f "$STAGE"' EXIT
python3 - "$PRIVATE" "$STAGE" <<'PY'
import os,pathlib,sys
source,destination=map(pathlib.Path,sys.argv[1:])
data=source.read_bytes()
descriptor=os.open(destination,os.O_WRONLY|os.O_CREAT|os.O_EXCL,0o600)
try:
    with os.fdopen(descriptor,"wb") as output: output.write(data); output.flush(); os.fsync(output.fileno())
except BaseException:
    try: os.unlink(destination)
    except OSError: pass
    raise
PY
python3 "$VERIFY" "$STAGE" --expected-commit "$COMMIT" >/dev/null; cmp -s "$PRIVATE" "$STAGE"
python3 - "$STAGE" "$PUBLIC" <<'PY'
import os,sys
os.link(sys.argv[1],sys.argv[2])
PY
if ! python3 "$VERIFY" "$PUBLIC" --expected-commit "$COMMIT" >/dev/null || ! cmp -s "$PRIVATE" "$PUBLIC"; then
  rm -f "$PUBLIC"
  exit 1
fi
