#!/usr/bin/env bash
# Fast hosted-contract checks; never execute the 60 rounds here.
set -euo pipefail
REPO="$(cd "$(dirname "$0")/../.." && pwd)"; RUN="$REPO/scripts/live-gates/run-qmp-stress-tier.sh"
VERIFY="$REPO/scripts/live-gates/verify-qmp-stress.py"; BASELINE="$REPO/scripts/live-gates/qmp-close-race-baseline.rs"
WORKFLOW="$REPO/.github/workflows/qmp-stress.yml"; CLI="$REPO/scripts/live-gates/bridgevm-live"
TIER="$REPO/scripts/live-gates/run-tier.sh"; SPECIAL="$REPO/scripts/live-gates/run-special-tier.sh"
python3 "$VERIFY" --self-test | grep -q PASS
grep -Fq 'qmp-stress:' "$WORKFLOW"; grep -Fq 'run-qmp-stress-tier.sh' "$WORKFLOW"
grep -Fq 'workflow_dispatch:' "$WORKFLOW"; grep -Fq 'github.event.repository.fork == false' "$WORKFLOW"
grep -Fq 'RUNNER_ENVIRONMENT:-}" == github-hosted' "$RUN"
grep -Fq 'for _ in $(seq 1 24)' "$RUN"; grep -Fq '/usr/bin/yes' "$RUN"; grep -Fq 'ps -o ppid=' "$RUN"; grep -Fq 'for round in $(seq 1 60)' "$RUN"
grep -Fq 'loads_alive ||' "$RUN"; [[ "$(grep -Fc 'loads_alive ||' "$RUN")" -eq 3 ]]
grep -Fq 'run-round --out "$OUT"' "$RUN"; grep -Fq 'cargo test --workspace --locked' "$REPO/scripts/live-gates/qmp_stress_contract.py"
! grep -Fq 't10-qmp-stress' "$CLI" "$TIER" "$SPECIAL"
! GITHUB_ACTIONS=false RUNNER_TEMP=/tmp "$RUN" 2>/dev/null
grep -Fq 'UnixListener' "$BASELINE"
! grep -Eq 'TcpListener|TcpStream|sleep [0-9]{3}|sudo|actions-runner' "$RUN" "$BASELINE"
bin="$(mktemp)"; trap 'rm -f "$bin"' EXIT; rustc "$BASELINE" -o "$bin"
[[ "$($bin 20)" == $'baseline_iterations=20\nbaseline_einval_then_econnrefused=20' ]]
echo 'PASS: hosted QMP 20/20 negative control and exact 60-round contract'
