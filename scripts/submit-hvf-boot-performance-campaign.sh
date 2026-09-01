#!/usr/bin/env bash
set -euo pipefail
REPO="$(cd "$(dirname "$0")/.." && pwd)"
CLI="$REPO/scripts/live-gates/bridgevm-live"
MODE=""; PAIRS=""; BASELINE=""; CANDIDATE=""; CAMPAIGN_ID=""; HARNESS_SHA=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --mode) MODE="$2"; shift 2 ;;
    --pairs) PAIRS="$2"; shift 2 ;;
    --baseline-manifest) BASELINE="$2"; shift 2 ;;
    --candidate-manifest) CANDIDATE="$2"; shift 2 ;;
    --campaign-id) CAMPAIGN_ID="$2"; shift 2 ;;
    --harness-sha) HARNESS_SHA="$2"; shift 2 ;;
    *) echo "unknown campaign option $1" >&2; exit 2 ;;
  esac
done
[[ "$MODE" =~ ^A[AB]$ && "$PAIRS" =~ ^[0-9]+$ && "$PAIRS" -ge 3 && -f "$BASELINE" ]] || {
  echo "usage: $0 --mode AA|AB --pairs N>=3 --baseline-manifest PATH [--candidate-manifest PATH]" >&2
  exit 2
}
if [[ "$MODE" == AB ]]; then
  [[ -f "$CANDIDATE" ]] || { echo "AB needs --candidate-manifest" >&2; exit 2; }
elif [[ -n "$CANDIDATE" ]]; then
  echo "AA uses the baseline manifest for both roles" >&2; exit 2
fi
[[ -n "$CAMPAIGN_ID" ]] || CAMPAIGN_ID="$(openssl rand -hex 16)"
[[ "$CAMPAIGN_ID" =~ ^[0-9a-f]{32}$ ]] || { echo "campaign id must be 32 lowercase hex characters" >&2; exit 2; }
[[ -n "$HARNESS_SHA" ]] || HARNESS_SHA="$(git -C "$REPO" rev-parse HEAD)"; [[ "$HARNESS_SHA" =~ ^[0-9a-f]{40}$ ]] || { echo "harness SHA must be 40 lowercase hex characters" >&2; exit 2; }
validate_base() {
  awk -F '\t' '
    $1 == "image" || $1 == "vars" || $1 == "binary" {
      if (NF != 3 || $2 !~ /^\// || $3 !~ /^[0-9a-f]{64}$/) exit 1; seen[$1]++; next
    }
    $1 == "binary_source_commit" || $1 == "binary_profile" ||
    $1 == "binary_features" || $1 == "rust_toolchain" {
      if (NF != 2 || $2 == "") exit 1; seen[$1]++; next
    }
    { exit 1 }
    END {
      split("image vars binary binary_source_commit binary_profile binary_features rust_toolchain", keys, " ")
      if (NR != 7) exit 1
      for (i in keys) if (seen[keys[i]] != 1) exit 1
    }' "$1"
}
validate_base "$BASELINE" || { echo "invalid baseline manifest" >&2; exit 2; }
[[ "$MODE" == AA ]] || validate_base "$CANDIDATE" || { echo "invalid candidate manifest" >&2; exit 2; }

umask 077
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
expected=$((PAIRS * 2))
printf 'campaign_id=%s\nmode=%s\nexpected_runs=%s\nharness_sha=%s\n' "$CAMPAIGN_ID" "$MODE" "$expected" "$HARNESS_SHA"
for ((ordinal = 1; ordinal <= expected; ordinal++)); do
  if (( ordinal % 2 == 1 )); then
    role=baseline; base="$BASELINE"
  else
    role=candidate; base="$BASELINE"
    [[ "$MODE" == AA ]] || base="$CANDIDATE"
  fi
  manifest="$WORK/manifest-$ordinal.tsv"
  cp "$base" "$manifest"
  printf 'campaign_id\t%s\ncampaign_mode\t%s\ncampaign_role\t%s\ncampaign_ordinal\t%s\ncampaign_expected_runs\t%s\n' \
    "$CAMPAIGN_ID" "$MODE" "$role" "$ordinal" "$expected" >> "$manifest"
  job="$("$CLI" submit t15-hvf-boot-performance --sha "$HARNESS_SHA" --input-manifest "$manifest")"
  printf 'ordinal=%s\trole=%s\tjob_id=%s\n' "$ordinal" "$role" "$job"
done
