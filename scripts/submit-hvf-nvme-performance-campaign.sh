#!/usr/bin/env bash
set -euo pipefail
export LC_ALL=C

REPO="$(cd "$(dirname "$0")/.." && pwd)"
CLI="$REPO/scripts/live-gates/bridgevm-live"
QUEUE_ROOT="${BRIDGEVM_LIVE_ROOT:-$HOME/BridgeVM/live-queue}"
MIN_FREE_GIB="${BRIDGEVM_LIVE_MIN_FREE_GIB:-100}"
MODE=""; PAIRS=""; BASELINE=""; CANDIDATE=""; CAMPAIGN_ID=""; HARNESS_SHA=""
VALIDATE_ONLY=0; STAGE=""

usage() {
  cat >&2 <<EOF
usage: $0 --mode AA|AB --pairs EVEN_N_10..100 --baseline-manifest PATH \\
  [--candidate-manifest PATH] [--campaign-id 32_HEX] [--harness-sha 40_HEX] \\
  [--queue-root PATH] [--validate-only]
EOF
}

fail() { echo "NVMe performance campaign: $*" >&2; exit 2; }
seal() { openssl dgst -sha256 -r "$1" | cut -d' ' -f1 | tr -d '\n'; }
value() { awk -F '\t' -v key="$1" '$1 == key { print $2; exit }' "$2"; }
hash() { awk -F '\t' -v key="$1" '$1 == key { print $3; exit }' "$2"; }

while [[ $# -gt 0 ]]; do
  case "$1" in
    --mode) [[ $# -ge 2 ]] || fail "--mode needs a value"; MODE="$2"; shift 2 ;;
    --pairs) [[ $# -ge 2 ]] || fail "--pairs needs a value"; PAIRS="$2"; shift 2 ;;
    --baseline-manifest) [[ $# -ge 2 ]] || fail "--baseline-manifest needs a path"; BASELINE="$2"; shift 2 ;;
    --candidate-manifest) [[ $# -ge 2 ]] || fail "--candidate-manifest needs a path"; CANDIDATE="$2"; shift 2 ;;
    --campaign-id) [[ $# -ge 2 ]] || fail "--campaign-id needs a value"; CAMPAIGN_ID="$2"; shift 2 ;;
    --harness-sha) [[ $# -ge 2 ]] || fail "--harness-sha needs a value"; HARNESS_SHA="$2"; shift 2 ;;
    --queue-root) [[ $# -ge 2 ]] || fail "--queue-root needs a path"; QUEUE_ROOT="$2"; shift 2 ;;
    --validate-only) VALIDATE_ONLY=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) fail "unknown option $1" ;;
  esac
done

[[ "$MODE" =~ ^A[AB]$ && "$PAIRS" =~ ^[1-9][0-9]*$ && -f "$BASELINE" && ! -L "$BASELINE" ]] || {
  usage
  exit 2
}
(( PAIRS >= 10 && PAIRS <= 100 && PAIRS % 2 == 0 )) || fail "--pairs must be even and in 10..100"
if [[ "$MODE" == AB ]]; then
  [[ -f "$CANDIDATE" && ! -L "$CANDIDATE" ]] || fail "AB needs a regular, non-symlink --candidate-manifest"
elif [[ -n "$CANDIDATE" ]]; then
  fail "AA uses the baseline manifest for both roles"
fi
[[ "$QUEUE_ROOT" == /* && "$QUEUE_ROOT" != / ]] || fail "queue root must be an absolute, non-root path"
[[ "$MIN_FREE_GIB" =~ ^(0|[1-9][0-9]{0,3})$ ]] && (( MIN_FREE_GIB <= 4096 )) \
  || fail "BRIDGEVM_LIVE_MIN_FREE_GIB must be a canonical integer in 0..4096"
[[ -n "$CAMPAIGN_ID" ]] || CAMPAIGN_ID="$(openssl rand -hex 16)"
[[ "$CAMPAIGN_ID" =~ ^[0-9a-f]{32}$ ]] || fail "campaign id must be 32 lowercase hex characters"
[[ -n "$HARNESS_SHA" ]] || HARNESS_SHA="$(git -C "$REPO" rev-parse HEAD)"
[[ "$HARNESS_SHA" =~ ^[0-9a-f]{40}$ ]] || fail "harness SHA must be 40 lowercase hex characters"
git -C "$REPO" cat-file -e "$HARNESS_SHA^{commit}" 2>/dev/null || fail "harness SHA is not an available commit"
[[ "$HARNESS_SHA" == "$(git -C "$REPO" rev-parse HEAD)" ]] || fail "harness SHA must be the current checked-out commit"

validate_base() {
  local manifest="$1" key path expected
  awk -F '\t' '
    $1 == "image" || $1 == "vars" || $1 == "binary" || $1 == "firmware" ||
    $1 == "renderer" || $1 == "config" || $1 == "workload_script" {
      if (NF != 3 || $2 !~ /^\// || $3 !~ /^[0-9a-f]{64}$/) exit 1
      seen[$1]++; next
    }
    $1 == "workload_profile" ||
    $1 == "file_mib" || $1 == "transfer_kib" ||
    $1 == "read_passes" || $1 == "write_passes" {
      if (NF != 2 || $2 == "") exit 1
      seen[$1]++; next
    }
    { exit 1 }
    END {
      split("image vars binary firmware renderer config workload_script workload_profile file_mib transfer_kib read_passes write_passes", keys, " ")
      if (NR != 12) exit 1
      for (i in keys) if (seen[keys[i]] != 1) exit 1
    }' "$manifest" || return 1
  for key in image vars; do
    path="$(value "$key" "$manifest")"
    [[ -f "$path" && ! -L "$path" ]] || return 1
  done
  # The physical tier hashes every lane clone before boot. Re-reading a huge
  # disk here would turn asynchronous submission into a long foreground test.
  for key in binary firmware renderer config workload_script; do
    path="$(value "$key" "$manifest")"; expected="$(hash "$key" "$manifest")"
    [[ -f "$path" && ! -L "$path" && "$(seal "$path")" == "$expected" ]] || return 1
  done
  python3 "$REPO/scripts/render-hvf-nvme-workload.py" \
    --verify-output "$(value workload_script "$manifest")" \
    --verify-config "$(value config "$manifest")" >/dev/null || return 1
  [[ "$(value workload_profile "$manifest")" == windows-nvme-warm-seq-v1 \
    && "$(value file_mib "$manifest")" == 512 \
    && "$(value transfer_kib "$manifest")" == 128 \
    && "$(value read_passes "$manifest")" == 5 \
    && "$(value write_passes "$manifest")" == 2 ]]
}

validate_base "$BASELINE" || fail "invalid baseline manifest or sealed inputs"
baseline_manifest_hash="$(seal "$BASELINE")"
if [[ "$MODE" == AB ]]; then
  validate_base "$CANDIDATE" || fail "invalid candidate manifest or sealed inputs"
  candidate_manifest_hash="$(seal "$CANDIDATE")"
  for key in image vars firmware renderer config workload_script workload_profile file_mib transfer_kib read_passes write_passes; do
    [[ "$(value "$key" "$BASELINE")" == "$(value "$key" "$CANDIDATE")" ]] || fail "$key path/value differs between A and B"
    case "$key" in
      image|vars|firmware|renderer|config|workload_script)
        [[ "$(hash "$key" "$BASELINE")" == "$(hash "$key" "$CANDIDATE")" ]] || fail "$key hash differs between A and B"
        ;;
    esac
  done
  [[ "$(hash binary "$BASELINE")" != "$(hash binary "$CANDIDATE")" ]] || fail "AB requires distinct baseline and candidate binary hashes"
fi

expected=$((PAIRS * 2))
(( expected >= 20 && expected <= 200 && expected % 4 == 0 )) || fail "derived run count is out of range"
role_for_ordinal() {
  local ordinal="$1"
  if (( ((((ordinal - 1) / 2) + ordinal) % 2) == 1 )); then
    printf 'baseline\n'
  else
    printf 'candidate\n'
  fi
}
job_for_ordinal() { printf 't16-%s-%03d\n' "$CAMPAIGN_ID" "$1"; }

printf 'campaign_id=%s\nmode=%s\npairs=%s\nexpected_runs=%s\nharness_sha=%s\n' \
  "$CAMPAIGN_ID" "$MODE" "$PAIRS" "$expected" "$HARNESS_SHA"
if [[ "$VALIDATE_ONLY" == 1 ]]; then
  for ((ordinal = 1; ordinal <= expected; ordinal++)); do
    role="$(role_for_ordinal "$ordinal")"; pair=$(((ordinal + 1) / 2))
    (( pair % 2 == 1 )) && order=AB || order=BA
    printf 'ordinal=%s\tpair=%s\torder=%s\trole=%s\n' "$ordinal" "$pair" "$order" "$role"
  done
  echo "validation=pass"
  exit 0
fi

mkdir -p "$QUEUE_ROOT"; QUEUE_ROOT="$(cd "$QUEUE_ROOT" && pwd -P)"; [[ "$QUEUE_ROOT" != / ]] || fail "physical queue root must not be /"

dirty_harness="$(git -C "$REPO" status --porcelain --untracked-files=all -- \
  .github/workflows/ci.yml scripts/hvf_nvme_performance_report.py scripts/render-hvf-nvme-workload.py \
  scripts/submit-hvf-nvme-performance-campaign.sh scripts/write-hvf-nvme-performance-receipt.py \
  scripts/live-gates/bridgevm-live scripts/live-gates/bridgevm-live-worker.sh \
  scripts/live-gates/redact-receipt.py \
  scripts/live-gates/hvf-nvme-performance-manifest.sh scripts/live-gates/run-hvf-nvme-performance-tier.sh \
  scripts/live-gates/run-special-tier.sh scripts/live-gates/run-tier.sh \
  tests/integration/hvf-nvme-performance-tier-smoke.sh)"
[[ -z "$dirty_harness" ]] || fail "T16 harness inputs must be committed before live submission"

space_path="$QUEUE_ROOT"
while [[ ! -e "$space_path" && "$space_path" != / ]]; do space_path="$(dirname "$space_path")"; done
[[ -e "$space_path" ]] || fail "cannot locate the queue filesystem"
queue_device="$(stat -f %d "$space_path" 2>/dev/null || true)"
[[ "$queue_device" =~ ^[0-9]+$ ]] || fail "cannot identify the queue filesystem"
manifests=("$BASELINE"); [[ "$MODE" == AA ]] || manifests+=("$CANDIDATE")
for manifest in "${manifests[@]}"; do
  for key in image vars; do
    [[ "$(stat -f %d "$(value "$key" "$manifest")" 2>/dev/null || true)" == "$queue_device" ]] \
      || fail "$key must be staged on the queue filesystem before cp -c"
  done
done
available_kib="$(df -Pk "$space_path" | awk 'END { print $4 }')"
[[ "$available_kib" =~ ^[0-9]+$ ]] || fail "cannot determine queue free space"
required_kib=$((MIN_FREE_GIB * 1024 * 1024 + expected * 512 * 2 * 1024))
(( available_kib >= required_kib )) || fail "queue has ${available_kib}KiB free; ${required_kib}KiB is required"

[[ -x "$CLI" ]] || fail "live queue CLI is not executable"
INPUT_ROOT="$QUEUE_ROOT/campaign-inputs"
FINAL="$INPUT_ROOT/$CAMPAIGN_ID"
[[ ! -e "$FINAL" ]] || fail "campaign input directory already exists"
mkdir -p "$INPUT_ROOT"
chmod 700 "$INPUT_ROOT"
STAGE="$QUEUE_ROOT/.nvme-campaign-staging-$CAMPAIGN_ID-$$"
mkdir -m 700 "$STAGE"
cleanup() {
  if [[ -n "$STAGE" && "$STAGE" == "$QUEUE_ROOT"/.nvme-campaign-staging-* && -d "$STAGE" ]]; then
    rm -rf -- "$STAGE"
  fi
}
trap cleanup EXIT

REGISTRY="$STAGE/campaign-registry.tsv"
printf 'schema\tbridgevm.t16-campaign-registry.v1\ncampaign_id\t%s\ncampaign_mode\t%s\npairs\t%s\nexpected_runs\t%s\nharness_commit\t%s\n' \
  "$CAMPAIGN_ID" "$MODE" "$PAIRS" "$expected" "$HARNESS_SHA" > "$REGISTRY"
for ((ordinal = 1; ordinal <= expected; ordinal++)); do
  printf 'lane\t%s\t%s\t%s\n' "$ordinal" "$(role_for_ordinal "$ordinal")" "$(job_for_ordinal "$ordinal")" >> "$REGISTRY"
done
chmod 400 "$REGISTRY"; REGISTRY_HASH="$(seal "$REGISTRY")"

for ((ordinal = 1; ordinal <= expected; ordinal++)); do
  role="$(role_for_ordinal "$ordinal")"; base="$BASELINE"
  [[ "$MODE" == AA || "$role" == baseline ]] || base="$CANDIDATE"
  lane="$STAGE/lane-$ordinal"; final_lane="$FINAL/lane-$ordinal"
  mkdir -m 700 "$lane"
  cp -c "$(value image "$base")" "$lane/disk.raw" || fail "lane $ordinal disk clone failed"
  cp -c "$(value vars "$base")" "$lane/vars.fd" || fail "lane $ordinal vars clone failed"
  chmod 600 "$lane/disk.raw" "$lane/vars.fd"
  available_kib="$(df -Pk "$STAGE" | awk 'END { print $4 }')"
  [[ "$available_kib" =~ ^[0-9]+$ && "$available_kib" -ge "$required_kib" ]] || fail "queue free-space guard failed after lane $ordinal"
  nonce="$(printf '%s:%s' "$CAMPAIGN_ID" "$ordinal" | openssl dgst -sha256 -r | cut -c1-32)"
  python3 "$REPO/scripts/render-hvf-nvme-workload.py" --output "$lane/nvme-workload.ps1" \
    --config-output "$lane/nvme-workload-config.json" --nonce "$nonce" >/dev/null \
    || fail "lane $ordinal workload render failed"
  [[ "$(seal "$lane/nvme-workload-config.json")" == "$(hash config "$base")" ]] \
    || fail "lane $ordinal canonical config differs from the sealed base"
  manifest="$lane/manifest.tsv"
  for key in binary firmware renderer; do
    printf '%s\t%s\t%s\n' "$key" "$(value "$key" "$base")" "$(hash "$key" "$base")" >> "$manifest"
  done
  printf 'config\t%s\t%s\nworkload_script\t%s\t%s\n' \
    "$final_lane/nvme-workload-config.json" "$(seal "$lane/nvme-workload-config.json")" \
    "$final_lane/nvme-workload.ps1" "$(seal "$lane/nvme-workload.ps1")" >> "$manifest"
  printf 'campaign_registry\t%s\t%s\n' "$FINAL/campaign-registry.tsv" "$REGISTRY_HASH" >> "$manifest"
  printf 'image\t%s\t%s\nvars\t%s\t%s\n' \
    "$final_lane/disk.raw" "$(hash image "$base")" "$final_lane/vars.fd" "$(hash vars "$base")" >> "$manifest"
  for key in workload_profile file_mib transfer_kib read_passes write_passes; do
    printf '%s\t%s\n' "$key" "$(value "$key" "$base")" >> "$manifest"
  done
  printf 'campaign_id\t%s\ncampaign_mode\t%s\ncampaign_role\t%s\ncampaign_ordinal\t%s\ncampaign_expected_runs\t%s\n' \
    "$CAMPAIGN_ID" "$MODE" "$role" "$ordinal" "$expected" >> "$manifest"
  chmod 600 "$manifest"
done

duplicate_media="$(find "$STAGE" -name manifest.tsv -exec awk -F '\t' '$1 == "image" || $1 == "vars" { print $1 "\t" $2 }' {} \; | sort | uniq -d)"
[[ -z "$duplicate_media" ]] || fail "campaign contains a shared writable disk or vars path"
validate_base "$BASELINE" || fail "baseline sealed inputs changed while lanes were staged"
[[ "$MODE" == AA ]] || validate_base "$CANDIDATE" || fail "candidate sealed inputs changed while lanes were staged"
[[ "$(seal "$BASELINE")" == "$baseline_manifest_hash" ]] || fail "baseline manifest changed while lanes were staged"
[[ "$MODE" == AA || "$(seal "$CANDIDATE")" == "$candidate_manifest_hash" ]] || fail "candidate manifest changed while lanes were staged"
mv "$STAGE" "$FINAL"
STAGE=""
for ((ordinal = 1; ordinal <= expected; ordinal++)); do
  role="$(role_for_ordinal "$ordinal")"
  expected_job="$(job_for_ordinal "$ordinal")"
  job="$(BRIDGEVM_LIVE_ROOT="$QUEUE_ROOT" "$CLI" submit t16-hvf-nvme-performance --job-id "$expected_job" --sha "$HARNESS_SHA" --input-manifest "$FINAL/lane-$ordinal/manifest.tsv")"
  [[ "$job" == "$expected_job" ]] || fail "queue returned an unexpected job id for ordinal $ordinal"
  printf 'ordinal=%s\trole=%s\tjob_id=%s\n' "$ordinal" "$role" "$job"
done
