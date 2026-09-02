#!/usr/bin/env bash
set -euo pipefail
[[ $# -eq 4 ]] || exit 2
DIR="$1"; WORKTREE="$2"; JOB_ID="$3"; COMMIT="$4"
PRIVATE="$DIR/private"; mkdir -p "$PRIVATE"
attempts=0
for ordinal in {1..10}; do
  [[ -d "$PRIVATE/lane-$ordinal" ]] || break
  attempts=$ordinal
done
started="$(awk -F= '$1=="started_at"{print $2}' "$DIR/job.env" | tail -1)"
[[ "$started" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$ ]] \
  || started="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
args=(--out "$DIR/receipt.json" --private "$PRIVATE" --manifest "$DIR/input-manifest.tsv"
  --sealed-binary "$DIR/hvf_gic_boot_probe" --job-id "$JOB_ID" --commit "$COMMIT"
  --started-at "$started" --attempts "$attempts" --outcome missing-receipt
  --failure-code missing-tier-receipt)
if [[ ! -e "$PRIVATE/media" ]] \
  && ! mount | grep -F "$DIR" >/dev/null 2>&1 \
  && ! pgrep -f "$DIR" >/dev/null 2>&1; then
  args+=(--cleanup-verified)
fi
python3 "$WORKTREE/scripts/live-gates/write-audio-teardown-receipt.py" "${args[@]}"
