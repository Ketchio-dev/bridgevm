#!/usr/bin/env bash
# t9-audio-teardown: ten independent playback + clean-shutdown runs.
set -euo pipefail
REPO="$(cd "$(dirname "$0")/../.." && pwd)"; OUT=""; JOB_ID="local-$(date +%Y%m%d-%H%M%S)"
while [[ $# -gt 0 ]]; do case "$1" in
  --out) OUT="$2"; shift 2;; --job-id) JOB_ID="$2"; shift 2;;
  *) echo "unknown audio-tier option $1" >&2; exit 2;; esac; done
[[ -n "$OUT" ]] || { echo 'audio tier needs --out' >&2; exit 2; }; mkdir -p "$OUT/runs"
TARGET=${TARGET:-$HOME/BridgeVM/work/net-live-20260724.raw}
VARS=${VARS:-$HOME/BridgeVM/work/net-live-20260724-vars.fd}; N=${N:-10}
for input in "$TARGET" "$VARS"; do head -c1 "$input" >/dev/null 2>&1 \
  || { echo "cannot read audio source: $input" >&2; exit 1; }; done
seal() { openssl dgst -sha256 -r "$1" | cut -d' ' -f1; }
image_hash=$(seal "$TARGET"); vars_hash=$(seal "$VARS"); passed=0; status=0
write_receipt() { local outcome="$1" pass="$2"; cat >"$OUT/receipt.json" <<EOF
{
  "tier": "t9-audio-teardown",
  "gate_id": "a5-audio-teardown-quality",
  "criterion": "A5-quality",
  "job_id": "$JOB_ID",
  "commit": "$(git -C "$REPO" rev-parse HEAD)",
  "image_sha256": "$image_hash",
  "vars_sha256": "$vars_hash",
  "host_model": "$(sysctl -n hw.model)",
  "macos_version": "$(sw_vers -productVersion)",
  "sample_count": $N,
  "finished_at": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "outcome": "$outcome",
  "pass": $pass
}
EOF
}
write_receipt failed false
for run in $(seq 1 "$N"); do
  run_out="$OUT/runs/run$run"; mkdir -p "$run_out"
  if OUT="$run_out" TARGET="$TARGET" VARS="$VARS" \
      bash "$REPO/scripts/verify-audio-playback.sh" >"$run_out/gate.log" 2>&1; then
    passed=$((passed + 1))
  else status=1; fi
done
printf 'passed %s/%s\n' "$passed" "$N" >"$OUT/summary.txt"
[[ "$passed" == "$N" ]] || status=1
write_receipt "$([[ $status == 0 ]] && echo completed || echo failed)" \
  "$([[ $status == 0 ]] && echo true || echo false)"
exit "$status"
