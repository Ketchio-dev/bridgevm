#!/usr/bin/env bash
# Reconcile jobs left in running after the sole queue worker exited.
# The caller must hold worker.lock, so every running entry is stale.
set -euo pipefail

REPO="${1:?recover-stale-jobs needs repo}"
QUEUE_ROOT="${2:?recover-stale-jobs needs queue root}"
WORK_ROOT="${3:?recover-stale-jobs needs work root}"
mkdir -p "$QUEUE_ROOT/done"

for dir in "$QUEUE_ROOT"/running/*; do
  [[ -d "$dir" ]] || continue
  job_id="$(awk -F= '$1=="job_id"{print $2}' "$dir/job.env" 2>/dev/null)"
  tier="$(awk -F= '$1=="tier"{print $2}' "$dir/job.env" 2>/dev/null)"
  commit="$(awk -F= '$1=="commit"{print $2}' "$dir/job.env" 2>/dev/null)"
  [[ -n "$job_id" && "$job_id" == "$(basename "$dir")" ]] || {
    echo "refusing malformed stale job: $dir" >&2
    continue
  }
  [[ ! -e "$QUEUE_ROOT/done/$job_id" ]] || {
    echo "refusing stale job with existing done entry: $job_id" >&2
    continue
  }
  if [[ ! -f "$dir/result.env" ]]; then
    printf 'result=interrupted-worker-exit\nexit_code=unknown\n' > "$dir/result.env"
  fi
  grep -q '^finished_at=' "$dir/job.env" || \
    printf 'finished_at=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" >> "$dir/job.env"
  "$REPO/scripts/live-gates/recover-stale-receipt.sh" \
    "$REPO" "$WORK_ROOT" "$dir" "$job_id" "$tier" "$commit"
  mv "$dir" "$QUEUE_ROOT/done/$job_id"
  echo "recovered stale job as interrupted: $job_id" >&2
done
