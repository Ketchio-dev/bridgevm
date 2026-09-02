#!/usr/bin/env bash
# Hosted-only QMP negative control plus exactly 60 loaded full-workspace rounds.
set -euo pipefail
REPO="$(cd "$(dirname "$0")/../.." && pwd)"; CONTRACT="$REPO/scripts/live-gates/qmp_stress_contract.py"
OUT=""; COMMIT=""; WORKFLOW_HEAD=""; RUN_ID=""; RUN_ATTEMPT=""; REPOSITORY=""
while (( $# )); do
  case "$1" in
    --out) [[ -z "$OUT" && $# -ge 2 ]] || exit 2; OUT="$2"; shift 2 ;;
    --commit) [[ -z "$COMMIT" && $# -ge 2 ]] || exit 2; COMMIT="$2"; shift 2 ;;
    --workflow-head-sha) [[ -z "$WORKFLOW_HEAD" && $# -ge 2 ]] || exit 2; WORKFLOW_HEAD="$2"; shift 2 ;;
    --run-id) [[ -z "$RUN_ID" && $# -ge 2 ]] || exit 2; RUN_ID="$2"; shift 2 ;;
    --run-attempt) [[ -z "$RUN_ATTEMPT" && $# -ge 2 ]] || exit 2; RUN_ATTEMPT="$2"; shift 2 ;;
    --repository) [[ -z "$REPOSITORY" && $# -ge 2 ]] || exit 2; REPOSITORY="$2"; shift 2 ;;
    *) exit 2 ;;
  esac
done
[[ "${GITHUB_ACTIONS:-}" == true && "${RUNNER_ENVIRONMENT:-}" == github-hosted && "${RUNNER_OS:-}" == macOS && "${GITHUB_EVENT_NAME:-}" == workflow_dispatch ]] || { echo 'hosted macOS workflow_dispatch only' >&2; exit 2; }
[[ "$OUT" == "${RUNNER_TEMP:?}"/* && ! -e "$OUT" && "$COMMIT" =~ ^[0-9a-f]{40}$ && "$WORKFLOW_HEAD" == "$COMMIT" && "$RUN_ID" =~ ^[1-9][0-9]*$ && "$RUN_ATTEMPT" =~ ^[1-9][0-9]*$ && "$REPOSITORY" =~ ^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$ && "${GITHUB_WORKFLOW_REF:-}" == "$REPOSITORY/.github/workflows/qmp-stress.yml@"* ]] || exit 2
[[ "$(git -C "$REPO" rev-parse HEAD)" == "$COMMIT" ]] || { echo 'commit does not match checkout HEAD' >&2; exit 2; }
git -C "$REPO" diff --quiet && git -C "$REPO" diff --cached --quiet || { echo 'tracked checkout is dirty' >&2; exit 2; }
mkdir -m 700 "$OUT" "$OUT/rounds"; printf '%s\n' $'schema\tround\tlog\traw_sha256\tgzip_sha256\traw_bytes\tgzip_bytes' >"$OUT/rounds.tsv"
STARTED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"; LOAD_PIDS=(); outcome=failed; failure_stage=load; campaign_start=$SECONDS
loads_alive() { local pid state owner command; for pid in "${LOAD_PIDS[@]}"; do kill -0 "$pid" 2>/dev/null || return 1; read -r owner command <<< "$(ps -o ppid= -o comm= -p "$pid" 2>/dev/null)"; state="$(ps -o stat= -p "$pid" 2>/dev/null | tr -d '[:space:]')"; [[ "$owner" == "$$" && "$command" == /usr/bin/yes && -n "$state" && "$state" != Z* ]] || return 1; done; }
finish() {
  status=$?; for pid in "${LOAD_PIDS[@]}"; do kill "$pid" 2>/dev/null || true; done; wait 2>/dev/null || true; rm -f "$OUT/qmp-close-race-baseline"
  if [[ ! -f "$OUT/receipt.json" ]]; then
    python3 "$CONTRACT" write-receipt --out "$OUT" --outcome "$outcome" --failure-stage "$failure_stage" --repository "$REPOSITORY" --commit "$COMMIT" --workflow-head-sha "$WORKFLOW_HEAD" --run-id "$RUN_ID" --run-attempt "$RUN_ATTEMPT" --started-at "$STARTED_AT" || status=1
  fi
  trap - EXIT; exit "$status"
}
trap finish EXIT; trap 'exit 130' INT TERM
for _ in $(seq 1 24); do /usr/bin/yes >/dev/null & LOAD_PIDS+=("$!"); done
loads_alive || { echo 'not all 24 load processes started' >&2; exit 1; }
failure_stage=baseline-build; rustc "$REPO/scripts/live-gates/qmp-close-race-baseline.rs" -o "$OUT/qmp-close-race-baseline" || exit $?
failure_stage=baseline; "$OUT/qmp-close-race-baseline" 20 >"$OUT/baseline.txt" || { rm -f "$OUT/baseline.txt"; exit 1; }; rm "$OUT/qmp-close-race-baseline"
for round in $(seq 1 60); do
  loads_alive || { failure_stage=load; echo 'load process exited before round' >&2; exit 1; }
  (( SECONDS - campaign_start < 9000 )) || { failure_stage=campaign-deadline; echo 'campaign deadline exceeded' >&2; exit 1; }; failure_stage="round-$(printf '%02d' "$round")"
  python3 "$CONTRACT" run-round --out "$OUT" --repo "$REPO" --round "$round" --timeout 900 || exit $?
  loads_alive || { failure_stage=load; echo 'load process exited during round' >&2; exit 1; }
done
failure_stage=source-mutation; [[ "$(git -C "$REPO" rev-parse HEAD)" == "$COMMIT" ]] && git -C "$REPO" diff --quiet && git -C "$REPO" diff --cached --quiet || exit 1
failure_stage=none; outcome=completed
