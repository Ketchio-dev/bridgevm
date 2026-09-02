#!/usr/bin/env bash
set -euo pipefail
export LC_ALL=C

REPO="$(cd "$(dirname "$0")/.." && pwd -P)"
source "$REPO/scripts/live-gates/hvf-nvme-performance-v2-manifest.sh"
readonly CLI="$REPO/scripts/live-gates/bridgevm-live"
readonly TIER="t16-hvf-nvme-performance"
readonly WORKLOAD_RENDERER="$REPO/scripts/render-hvf-nvme-workload-v2.py"
readonly SPACE_RESERVE_GIB=128

QUEUE_ROOT="${BRIDGEVM_LIVE_ROOT:-$HOME/BridgeVM/live-queue}"
BASE_MANIFEST="" PUBLIC_SEED="" HARNESS_SHA="" VALIDATE_ONLY=0 STAGE=""
BASE_SET=0 SEED_SET=0 HARNESS_SET=0 QUEUE_SET=0 VALIDATE_SET=0

usage() {
  cat >&2 <<EOF
usage: $0 --base-manifest PATH --public-seed 64_LOWERCASE_HEX \\
  --harness-sha EXACT_CURRENT_40_HEX [--queue-root PATH] [--validate-only]
       $0 --self-test
EOF
}

fail() { printf 'NVMe calibration v2 campaign: %s\n' "$*" >&2; exit 2; }

schedule() {
  python3 - "$1" <<'PY'
import hashlib, sys
seed = sys.argv[1]
ranked = sorted((hashlib.sha256(f"{seed}:{pair}".encode()).hexdigest(), pair)
                for pair in range(1, 25))
ab = {pair for _, pair in ranked[:12]}
for pair in range(1, 25):
    print(f"{pair}\t{'AB' if pair in ab else 'BA'}")
PY
}

campaign_id() {
  printf 'bridgevm-t16-nvme-v2:%s:%s' "$1" "$2" \
    | openssl dgst -sha256 -r | cut -c1-32
}

job_id() { printf 't16-%s-%03d\n' "$1" "$2"; }

required_kib() {
  # Each of 48 lanes may consume one 2 GiB benchmark file in the campaign
  # clone and one in its live-job clone, plus a non-negotiable 128 GiB reserve.
  printf '%s\n' "$((SPACE_RESERVE_GIB * 1024 * 1024 + NVME_PERF_V2_RUNS * NVME_PERF_V2_FILE_MIB * 2 * 1024))"
}

write_registry() {
  local path="$1" seed_path="$2" campaign="$3" harness="$4" binary_hash="$5"
  local seed pair order label ordinal=0
  seed="$(tr -d '\n' < "$seed_path")"
  printf 'schema\t%s\ncampaign_id\t%s\ncampaign_mode\tAA\nworkload_profile\t%s\n' \
    "$NVME_PERF_V2_REGISTRY_SCHEMA" "$campaign" "$NVME_PERF_V2_PROFILE" > "$path"
  printf 'pairs\t24\nexpected_runs\t48\nharness_commit\t%s\npublic_seed\t%s\n' \
    "$harness" "$seed" >> "$path"
  printf 'public_seed_sha256\t%s\nbinary_a_sha256\t%s\nbinary_b_sha256\t%s\n' \
    "$(nvme_perf_v2_seal "$seed_path")" "$binary_hash" "$binary_hash" >> "$path"
  printf '%s\n' $'order_algorithm\tsha256-balanced-rank-v1' \
    $'replacement_policy\tforbidden' $'optional_stopping\tforbidden' \
    $'file_mib\t2048' $'transfer_kib\t128' $'read_passes\t16' $'write_passes\t4' \
    $'queue_depth\t1' $'post_warmup_settle_seconds\t30' \
    $'verification_timing\toutside-timed-read' >> "$path"
  while IFS=$'\t' read -r pair order; do
    for label in "${order:0:1}" "${order:1:1}"; do
      ordinal=$((ordinal + 1))
      printf 'lane\t%s\t%s\t%s\t%s\t%s\t%s\n' "$ordinal" "$pair" "$order" "$label" \
        "$binary_hash" "$(job_id "$campaign" "$ordinal")" >> "$path"
    done
  done < <(schedule "$seed")
}

assert_independent_media() {
  local root="$1" path inode device="" current
  : > "$root/media-inodes.tsv"
  for path in "$root"/lane-*/disk.raw "$root"/lane-*/vars.fd; do
    [[ -f "$path" && ! -L "$path" ]] || return 1
    inode="$(stat -f %i "$path" 2>/dev/null || true)"
    current="$(stat -f %d "$path" 2>/dev/null || true)"
    [[ "$inode" =~ ^[0-9]+$ && "$current" =~ ^[0-9]+$ ]] || return 1
    [[ -z "$device" || "$current" == "$device" ]] || return 1
    device="$current"
    printf '%s\t%s\n' "$current" "$inode" >> "$root/media-inodes.tsv"
  done
  [[ "$(wc -l < "$root/media-inodes.tsv" | tr -d ' ')" == 96 \
    && -z "$(sort "$root/media-inodes.tsv" | uniq -d)" ]]
}

self_test() (
  set -euo pipefail
  local temporary seed campaign harness registry binary_hash order_output key path base output head
  temporary="$(mktemp -d "${TMPDIR:-/tmp}/bridgevm-nvme-v2-submit.XXXXXX")"
  temporary="$(cd "$temporary" && pwd -P)"
  trap 'rm -rf "$temporary"' EXIT
  seed="$(printf '42%.0s' {1..32})"
  harness=0123456789abcdef0123456789abcdef01234567
  campaign="$(campaign_id "$seed" "$harness")"
  printf '%s\n' "$seed" > "$temporary/public-seed.txt"
  printf 'same-binary-for-a-and-b\n' > "$temporary/binary"
  binary_hash="$(nvme_perf_v2_seal "$temporary/binary")"
  registry="$temporary/registry.tsv"
  write_registry "$registry" "$temporary/public-seed.txt" "$campaign" "$harness" "$binary_hash"
  nvme_perf_v2_registry_validate "$registry" "$temporary/public-seed.txt" "$campaign" "$harness" "$binary_hash"
  order_output="$(schedule "$seed")"
  [[ "$(grep -c $'\tAB$' <<< "$order_output")" == 12 \
    && "$(grep -c $'\tBA$' <<< "$order_output")" == 12 \
    && "$(grep -c '^lane' "$registry")" == 48 \
    && "$(required_kib)" == 335544320 ]]
  mkdir "$temporary/lanes"; printf 'disk\n' > "$temporary/disk"; printf 'vars\n' > "$temporary/vars"
  for ordinal in {1..48}; do
    mkdir "$temporary/lanes/lane-$ordinal"
    cp "$temporary/disk" "$temporary/lanes/lane-$ordinal/disk.raw"
    cp "$temporary/vars" "$temporary/lanes/lane-$ordinal/vars.fd"
  done
  assert_independent_media "$temporary/lanes"
  printf 'mutated\n' >> "$temporary/public-seed.txt"
  ! nvme_perf_v2_registry_validate "$registry" "$temporary/public-seed.txt" "$campaign" "$harness" "$binary_hash"
  printf '%s\n' "$seed" > "$temporary/public-seed.txt"
  for key in image vars firmware renderer; do printf 'fixture-%s\n' "$key" > "$temporary/source-$key"; done
  python3 "$WORKLOAD_RENDERER" --output "$temporary/base-workload.ps1" \
    --config-output "$temporary/base-config.json" --nonce 0123456789abcdef0123456789abcdef >/dev/null
  base="$temporary/base.tsv"; : > "$base"
  for key in image vars binary firmware renderer; do
    [[ "$key" == binary ]] && path="$temporary/binary" || path="$temporary/source-$key"
    printf '%s\t%s\t%s\n' "$key" "$path" "$(nvme_perf_v2_seal "$path")" >> "$base"
  done
  for key in config workload_script quiescence_script quiescence_config environment_policy; do
    case "$key" in
      config) path="$temporary/base-config.json" ;;
      workload_script) path="$temporary/base-workload.ps1" ;;
      quiescence_script) path="$REPO/scripts/win-assets/bv-nvme-quiescence-v2.ps1" ;;
      quiescence_config) path="$REPO/scripts/live-gates/hvf-nvme-performance-v2-quiescence.json" ;;
      environment_policy) path="$REPO/scripts/live-gates/hvf-nvme-performance-v2-environment.sh" ;;
    esac
    printf '%s\t%s\t%s\n' "$key" "$path" "$(nvme_perf_v2_seal "$path")" >> "$base"
  done
  printf '%s\n' $'schema\tbridgevm.t16-nvme-calibration-base.v2' \
    $'workload_profile\twindows-nvme-warm-seq-v2' $'file_mib\t2048' \
    $'transfer_kib\t128' $'read_passes\t16' $'write_passes\t4' $'queue_depth\t1' \
    $'post_warmup_settle_seconds\t30' $'verification_timing\toutside-timed-read' >> "$base"
  head="$(git -C "$REPO" rev-parse HEAD)"
  output="$("$REPO/scripts/submit-hvf-nvme-calibration-v2-campaign.sh" \
    --base-manifest "$base" --public-seed "$seed" --harness-sha "$head" \
    --queue-root "$temporary/unused-queue" --validate-only)"
  [[ "$(grep -c '^pair=' <<< "$output")" == 48 \
    && "$(grep -c $'order=AB\t' <<< "$output")" == 24 \
    && "$(grep -c $'order=BA\t' <<< "$output")" == 24 \
    && "$output" == *'required_free_kib=335544320'* && "$output" == *'validation=pass'* ]]
  printf 'input-mutation\n' >> "$temporary/base-config.json"
  ! "$REPO/scripts/submit-hvf-nvme-calibration-v2-campaign.sh" \
    --base-manifest "$base" --public-seed "$seed" --harness-sha "$head" \
    --queue-root "$temporary/unused-queue" --validate-only >/dev/null 2>&1
  printf '%s\n' 'HVF NVMe calibration v2 submission self-test: PASS'
)

if [[ "${1:-}" == --self-test ]]; then
  [[ $# -eq 1 ]] || fail "--self-test takes no other arguments"
  self_test
  exit 0
fi

while [[ $# -gt 0 ]]; do
  case "$1" in
    --base-manifest) [[ $# -ge 2 && "$BASE_SET" == 0 ]] || fail "--base-manifest needs one value"; BASE_MANIFEST="$2"; BASE_SET=1; shift 2 ;;
    --public-seed) [[ $# -ge 2 && "$SEED_SET" == 0 ]] || fail "--public-seed needs one value"; PUBLIC_SEED="$2"; SEED_SET=1; shift 2 ;;
    --harness-sha) [[ $# -ge 2 && "$HARNESS_SET" == 0 ]] || fail "--harness-sha needs one value"; HARNESS_SHA="$2"; HARNESS_SET=1; shift 2 ;;
    --queue-root) [[ $# -ge 2 && "$QUEUE_SET" == 0 ]] || fail "--queue-root needs one path"; QUEUE_ROOT="$2"; QUEUE_SET=1; shift 2 ;;
    --validate-only) [[ "$VALIDATE_SET" == 0 ]] || fail "--validate-only may appear only once"; VALIDATE_ONLY=1; VALIDATE_SET=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) fail "unknown option $1" ;;
  esac
done

[[ -n "$BASE_MANIFEST" && "$PUBLIC_SEED" =~ ^[0-9a-f]{64}$ \
  && "$HARNESS_SHA" =~ ^[0-9a-f]{40}$ ]] || { usage; exit 2; }
[[ "$QUEUE_ROOT" == /* && "$QUEUE_ROOT" != / ]] || fail "queue root must be an absolute, non-root path"
git -C "$REPO" cat-file -e "$HARNESS_SHA^{commit}" 2>/dev/null \
  || fail "harness SHA is not an available commit"
[[ "$HARNESS_SHA" == "$(git -C "$REPO" rev-parse HEAD)" ]] \
  || fail "harness SHA must equal the current checkout"
nvme_perf_v2_base_validate "$REPO" "$BASE_MANIFEST" \
  || fail "base manifest, v2 geometry, or sealed input is invalid"

CAMPAIGN_ID="$(campaign_id "$PUBLIC_SEED" "$HARNESS_SHA")"
printf 'campaign_id=%s\nmode=AA\npairs=24\nexpected_runs=48\nharness_sha=%s\nrequired_free_kib=%s\n' \
  "$CAMPAIGN_ID" "$HARNESS_SHA" "$(required_kib)"
while IFS=$'\t' read -r pair order; do
  first=$((pair * 2 - 1)); second=$((pair * 2))
  printf 'pair=%s\torder=%s\tordinal=%s\tlabel=%s\n' "$pair" "$order" "$first" "${order:0:1}"
  printf 'pair=%s\torder=%s\tordinal=%s\tlabel=%s\n' "$pair" "$order" "$second" "${order:1:1}"
done < <(schedule "$PUBLIC_SEED")
if [[ "$VALIDATE_ONLY" == 1 ]]; then
  printf '%s\n' 'validation=pass'
  exit 0
fi

readonly HARNESS_FILES=(
  .github/workflows/ci.yml
  scripts/hvf_nvme_performance_v2_report.py
  scripts/render-hvf-nvme-workload-v2.py
  scripts/submit-hvf-nvme-calibration-v2-campaign.sh
  scripts/verify-hvf-nvme-quiescence-v2.py
  scripts/write-hvf-nvme-performance-v2-receipt.py
  scripts/win-assets/bv-nvme-quiescence-v2.ps1
  scripts/live-gates/bridgevm-live
  scripts/live-gates/bridgevm-live-worker.sh
  scripts/live-gates/hvf-nvme-performance-runtime.sh
  scripts/live-gates/hvf-nvme-performance-v2-environment.py
  scripts/live-gates/hvf-nvme-performance-v2-environment.sh
  scripts/live-gates/hvf-nvme-performance-v2-manifest.sh
  scripts/live-gates/hvf-nvme-performance-v2-quiescence.json
  scripts/live-gates/live-process-cleanup.sh
  scripts/live-gates/redact-receipt.py
  scripts/live-gates/run-hvf-nvme-performance-v1-tier.sh
  scripts/live-gates/run-hvf-nvme-performance-v2-tier.sh
  scripts/live-gates/run-hvf-nvme-performance-tier.sh
  scripts/live-gates/run-special-tier.sh
  scripts/live-gates/run-tier.sh
)
for tracked in "${HARNESS_FILES[@]}"; do
  git -C "$REPO" ls-files --error-unmatch "$tracked" >/dev/null 2>&1 \
    || fail "harness input is not committed: $tracked"
done
[[ -z "$(git -C "$REPO" status --porcelain --untracked-files=all -- "${HARNESS_FILES[@]}")" ]] \
  || fail "T16 v2 harness inputs must be clean at the exact harness SHA"

# The sealed policy owns macOS-specific observation. It prints diagnostics and
# returns nonzero unless AC, thermal nominal and >=300 s HID idle all hold.
ENVIRONMENT_POLICY="$(nvme_perf_v2_value environment_policy "$BASE_MANIFEST")"
source "$ENVIRONMENT_POLICY"
declare -F hvf_nvme_v2_environment_preflight >/dev/null \
  || fail "sealed environment policy lacks hvf_nvme_v2_environment_preflight"
hvf_nvme_v2_environment_preflight \
  || fail "current host is ineligible for an immutable 48-run calibration"

space_path="$QUEUE_ROOT"
while [[ ! -e "$space_path" && "$space_path" != / ]]; do space_path="$(dirname "$space_path")"; done
[[ -e "$space_path" ]] || fail "cannot locate the queue filesystem"
queue_device="$(stat -f %d "$space_path" 2>/dev/null || true)"
[[ "$queue_device" =~ ^[0-9]+$ ]] || fail "cannot identify the queue filesystem"
for key in image vars; do
  [[ "$(stat -f %d "$(nvme_perf_v2_value "$key" "$BASE_MANIFEST")" 2>/dev/null || true)" == "$queue_device" ]] \
    || fail "$key must already be staged on the queue filesystem for cp -c"
done
available_kib="$(df -Pk "$space_path" | awk 'END { print $4 }')"
[[ "$available_kib" =~ ^[0-9]+$ && "$available_kib" -ge "$(required_kib)" ]] \
  || fail "queue needs at least $(required_kib)KiB (320GiB) free"
[[ -x "$CLI" ]] || fail "live queue CLI is not executable"

mkdir -p "$QUEUE_ROOT"; QUEUE_ROOT="$(cd "$QUEUE_ROOT" && pwd -P)"
INPUT_ROOT="$QUEUE_ROOT/campaign-inputs-v2"; FINAL="$INPUT_ROOT/$CAMPAIGN_ID"
[[ ! -e "$FINAL" ]] || fail "campaign is immutable and already exists"
for ordinal in {1..48}; do
  expected_job="$(job_id "$CAMPAIGN_ID" "$ordinal")"
  [[ ! -e "$QUEUE_ROOT/job-ledger/$expected_job" && ! -e "$QUEUE_ROOT/queued/$expected_job" \
    && ! -e "$QUEUE_ROOT/running/$expected_job" && ! -e "$QUEUE_ROOT/done/$expected_job" ]] \
    || fail "campaign job id is already burned: $expected_job"
done
mkdir -p "$INPUT_ROOT"; chmod 700 "$INPUT_ROOT"
STAGE="$QUEUE_ROOT/.nvme-v2-campaign-staging-$CAMPAIGN_ID-$$"
mkdir -m 700 "$STAGE"
cleanup() {
  if [[ -n "$STAGE" && "$STAGE" == "$QUEUE_ROOT"/.nvme-v2-campaign-staging-* && -d "$STAGE" ]]; then
    rm -rf -- "$STAGE"
  fi
}
trap cleanup EXIT

printf '%s\n' "$PUBLIC_SEED" > "$STAGE/public-seed.txt"; chmod 400 "$STAGE/public-seed.txt"
mkdir -m 700 "$STAGE/common"
cp "$(nvme_perf_v2_value binary "$BASE_MANIFEST")" "$STAGE/common/hvf_gic_boot_probe"
chmod 500 "$STAGE/common/hvf_gic_boot_probe"
BINARY_HASH="$(nvme_perf_v2_hash binary "$BASE_MANIFEST")"
[[ "$(nvme_perf_v2_seal "$STAGE/common/hvf_gic_boot_probe")" == "$BINARY_HASH" ]] \
  || fail "binary changed while the campaign copy was sealed"
REGISTRY="$STAGE/campaign-registry.tsv"
write_registry "$REGISTRY" "$STAGE/public-seed.txt" "$CAMPAIGN_ID" "$HARNESS_SHA" "$BINARY_HASH"
nvme_perf_v2_registry_validate "$REGISTRY" "$STAGE/public-seed.txt" "$CAMPAIGN_ID" "$HARNESS_SHA" "$BINARY_HASH" \
  || fail "generated v2 registry failed its strict validator"
chmod 400 "$REGISTRY"; REGISTRY_HASH="$(nvme_perf_v2_seal "$REGISTRY")"
BASE_MANIFEST_HASH="$(nvme_perf_v2_seal "$BASE_MANIFEST")"
ordinal=0
while IFS=$'\t' read -r pair order; do
  for label in "${order:0:1}" "${order:1:1}"; do
    ordinal=$((ordinal + 1)); lane="$STAGE/lane-$ordinal"; final_lane="$FINAL/lane-$ordinal"
    mkdir -m 700 "$lane"
    cp -c "$(nvme_perf_v2_value image "$BASE_MANIFEST")" "$lane/disk.raw" \
      || fail "lane $ordinal disk clone failed"
    cp -c "$(nvme_perf_v2_value vars "$BASE_MANIFEST")" "$lane/vars.fd" \
      || fail "lane $ordinal vars clone failed"
    chmod 600 "$lane/disk.raw" "$lane/vars.fd"
    nonce="$(nvme_perf_v2_expected_nonce "$PUBLIC_SEED" "$CAMPAIGN_ID" "$ordinal")"
    python3 "$WORKLOAD_RENDERER" --output "$lane/nvme-workload.ps1" \
      --config-output "$lane/nvme-workload-config.json" --nonce "$nonce" \
      >/dev/null \
      || fail "lane $ordinal v2 workload render failed"
    [[ "$(nvme_perf_v2_seal "$lane/nvme-workload-config.json")" == "$(nvme_perf_v2_hash config "$BASE_MANIFEST")" ]] \
      || fail "lane $ordinal v2 config differs from the sealed base"
    manifest="$lane/manifest.tsv"
    printf 'image\t%s\t%s\nvars\t%s\t%s\nbinary\t%s\t%s\n' \
      "$final_lane/disk.raw" "$(nvme_perf_v2_hash image "$BASE_MANIFEST")" \
      "$final_lane/vars.fd" "$(nvme_perf_v2_hash vars "$BASE_MANIFEST")" \
      "$FINAL/common/hvf_gic_boot_probe" "$BINARY_HASH" > "$manifest"
    for key in firmware renderer; do
      printf '%s\t%s\t%s\n' "$key" "$(nvme_perf_v2_value "$key" "$BASE_MANIFEST")" \
        "$(nvme_perf_v2_hash "$key" "$BASE_MANIFEST")" >> "$manifest"
    done
    for key in quiescence_script quiescence_config environment_policy; do
      asset="$(nvme_perf_v2_value "$key" "$BASE_MANIFEST")"
      cp "$asset" "$lane/$(basename "$asset")"
      chmod 500 "$lane/$(basename "$asset")"
      [[ "$key" == quiescence_config ]] && chmod 400 "$lane/$(basename "$asset")"
      printf '%s\t%s\t%s\n' "$key" "$final_lane/$(basename "$asset")" \
        "$(nvme_perf_v2_hash "$key" "$BASE_MANIFEST")" >> "$manifest"
    done
    printf 'config\t%s\t%s\nworkload_script\t%s\t%s\n' \
      "$final_lane/nvme-workload-config.json" "$(nvme_perf_v2_seal "$lane/nvme-workload-config.json")" \
      "$final_lane/nvme-workload.ps1" "$(nvme_perf_v2_seal "$lane/nvme-workload.ps1")" >> "$manifest"
    printf 'campaign_registry\t%s\t%s\npublic_seed\t%s\t%s\n' \
      "$FINAL/campaign-registry.tsv" "$REGISTRY_HASH" "$FINAL/public-seed.txt" \
      "$(nvme_perf_v2_seal "$STAGE/public-seed.txt")" >> "$manifest"
    printf '%s\n' $'schema\tbridgevm.t16-nvme-calibration-lane.v2' $'campaign_mode\tAA' \
      $'campaign_expected_runs\t48' $'workload_profile\twindows-nvme-warm-seq-v2' $'file_mib\t2048' \
      $'transfer_kib\t128' $'read_passes\t16' $'write_passes\t4' $'queue_depth\t1' \
      $'post_warmup_settle_seconds\t30' $'verification_timing\toutside-timed-read' \
      $'replacement_policy\tforbidden' $'optional_stopping\tforbidden' >> "$manifest"
    printf 'campaign_id\t%s\ncampaign_label\t%s\ncampaign_order\t%s\n' \
      "$CAMPAIGN_ID" "$label" "$order" >> "$manifest"
    printf 'campaign_pair\t%s\ncampaign_ordinal\t%s\nharness_commit\t%s\nworkload_nonce\t%s\n' \
      "$pair" "$ordinal" "$HARNESS_SHA" "$nonce" >> "$manifest"
    chmod 600 "$manifest"
  done
done < <(schedule "$PUBLIC_SEED")
[[ "$ordinal" == 48 ]] || fail "internal schedule did not create exactly 48 lanes"
assert_independent_media "$STAGE" || fail "disk and vars clones are not 96 independent files"
rm "$STAGE/media-inodes.tsv"

# Re-read every source and the source manifest after all clone/render work. A
# mutation invalidates the whole campaign before any immutable job id is burned.
nvme_perf_v2_base_validate "$REPO" "$BASE_MANIFEST" \
  || fail "a sealed base input changed while lanes were staged"
[[ "$(nvme_perf_v2_seal "$BASE_MANIFEST")" == "$BASE_MANIFEST_HASH" ]] \
  || fail "base manifest changed while lanes were staged"
[[ "$(nvme_perf_v2_seal "$STAGE/common/hvf_gic_boot_probe")" == "$BINARY_HASH" \
  && "$(nvme_perf_v2_seal "$STAGE/public-seed.txt")" == "$(nvme_perf_v2_hash public_seed "$STAGE/lane-1/manifest.tsv")" \
  && "$(nvme_perf_v2_seal "$REGISTRY")" == "$REGISTRY_HASH" ]] \
  || fail "a campaign-owned sealed input changed while lanes were staged"

mv "$STAGE" "$FINAL"; STAGE=""
for ordinal in {1..48}; do
  nvme_perf_v2_lane_validate "$REPO" "$FINAL/lane-$ordinal/manifest.tsv" \
    "$FINAL/common/hvf_gic_boot_probe" \
    || fail "final lane $ordinal failed the strict v2 manifest validator"
done
for ordinal in {1..48}; do
  expected_job="$(job_id "$CAMPAIGN_ID" "$ordinal")"
  job="$(BRIDGEVM_LIVE_ROOT="$QUEUE_ROOT" "$CLI" submit "$TIER" --job-id "$expected_job" \
    --sha "$HARNESS_SHA" --input-manifest "$FINAL/lane-$ordinal/manifest.tsv")"
  [[ "$job" == "$expected_job" ]] || fail "queue returned an unexpected job id for ordinal $ordinal"
  printf 'ordinal=%s\tjob_id=%s\n' "$ordinal" "$job"
done
