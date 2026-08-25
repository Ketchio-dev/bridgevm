#!/usr/bin/env bash
# Fast contract checks; never execute the 60-round tier here.
set -euo pipefail
REPO="$(cd "$(dirname "$0")/../.." && pwd)"
RUN="$REPO/scripts/live-gates/run-qmp-stress-tier.sh"
VERIFY="$REPO/scripts/live-gates/verify-qmp-stress.py"
BASELINE="$REPO/scripts/live-gates/qmp-close-race-baseline.rs"
CLI="$REPO/scripts/live-gates/bridgevm-live"
TIER="$REPO/scripts/live-gates/run-tier.sh"
[[ -x "$RUN" && -x "$VERIFY" ]]
python3 "$VERIFY" --self-test | grep -q PASS
grep -Fq 'for _ in $(seq 1 24)' "$RUN"
grep -Fq 'for round in $(seq 1 60)' "$RUN"
grep -Fq 'cargo test --workspace --locked' "$RUN"
grep -Fq 'verify-qmp-stress.py" --out "$OUT"' "$RUN"
grep -Fq 't10-qmp-stress)' "$CLI"
grep -Fq 'run-qmp-stress-tier.sh' "$TIER"
grep -Fq 'UnixListener' "$BASELINE"
! grep -Eq 'TcpListener|TcpStream|sleep [0-9]{3}|sudo|actions-runner' "$RUN" "$BASELINE"
bin="$(mktemp)"; trap 'rm -f "$bin"' EXIT
rustc "$BASELINE" -o "$bin"
[[ "$($bin 20)" == $'baseline_iterations=20\nbaseline_einval_then_econnrefused=20' ]]
echo 'PASS: QMP full-workspace stress tier contract and negative control'
