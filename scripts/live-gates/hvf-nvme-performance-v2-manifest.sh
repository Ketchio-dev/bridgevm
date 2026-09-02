#!/usr/bin/env bash

# Strict, separately versioned input contract for T16 NVMe calibration v2.

readonly NVME_PERF_V2_BASE_SCHEMA="bridgevm.t16-nvme-calibration-base.v2"
readonly NVME_PERF_V2_LANE_SCHEMA="bridgevm.t16-nvme-calibration-lane.v2"
readonly NVME_PERF_V2_REGISTRY_SCHEMA="bridgevm.t16-nvme-calibration-registry.v2"
readonly NVME_PERF_V2_PROFILE="windows-nvme-warm-seq-v2"
readonly NVME_PERF_V2_PAIRS=24 NVME_PERF_V2_RUNS=48
readonly NVME_PERF_V2_FILE_MIB=2048 NVME_PERF_V2_TRANSFER_KIB=128
readonly NVME_PERF_V2_READ_PASSES=16 NVME_PERF_V2_WRITE_PASSES=4
readonly NVME_PERF_V2_QUEUE_DEPTH=1 NVME_PERF_V2_SETTLE_SECONDS=30
readonly NVME_PERF_V2_VERIFICATION_TIMING="outside-timed-read"

nvme_perf_v2_seal() {
  openssl dgst -sha256 -r "$1" 2>/dev/null | cut -d' ' -f1 | tr -d '\n'
}

nvme_perf_v2_value() {
  LC_ALL=C awk -F '\t' -v key="$1" '$1 == key { print $2; exit }' "$2"
}

nvme_perf_v2_hash() {
  LC_ALL=C awk -F '\t' -v key="$1" '$1 == key { print $3; exit }' "$2"
}

nvme_perf_v2_regular() {
  local path="$1" parent
  [[ "$path" == /* && -f "$path" && ! -L "$path" ]] || return 1
  parent="$(cd "$(dirname "$path")" && pwd -P)" || return 1
  [[ "$path" == "$parent/$(basename "$path")" ]]
}

nvme_perf_v2_base_structure() {
  LC_ALL=C awk -F '\t' '
    $1 == "image" || $1 == "vars" || $1 == "binary" ||
    $1 == "firmware" || $1 == "renderer" || $1 == "config" ||
    $1 == "workload_script" || $1 == "quiescence_script" ||
    $1 == "quiescence_config" || $1 == "environment_policy" {
      if (NF != 3 || $2 !~ /^\// || $3 !~ /^[0-9a-f]{64}$/) exit 1
      seen[$1]++; next
    }
    $1 == "schema" || $1 == "workload_profile" ||
    $1 == "file_mib" || $1 == "transfer_kib" ||
    $1 == "read_passes" || $1 == "write_passes" ||
    $1 == "queue_depth" || $1 == "post_warmup_settle_seconds" ||
    $1 == "verification_timing" {
      if (NF != 2 || $2 == "") exit 1
      seen[$1]++; next
    }
    { exit 1 }
    END {
      split("image vars binary firmware renderer config workload_script quiescence_script quiescence_config environment_policy schema workload_profile file_mib transfer_kib read_passes write_passes queue_depth post_warmup_settle_seconds verification_timing", keys, " ")
      if (NR != 19) exit 1
      for (i in keys) if (seen[keys[i]] != 1) exit 1
    }' "$1"
}

nvme_perf_v2_lane_structure() {
  LC_ALL=C awk -F '\t' '
    $1 == "image" || $1 == "vars" || $1 == "binary" ||
    $1 == "firmware" || $1 == "renderer" || $1 == "config" ||
    $1 == "workload_script" || $1 == "quiescence_script" ||
    $1 == "quiescence_config" || $1 == "environment_policy" ||
    $1 == "campaign_registry" || $1 == "public_seed" {
      if (NF != 3 || $2 !~ /^\// || $3 !~ /^[0-9a-f]{64}$/) exit 1
      seen[$1]++; next
    }
    $1 == "schema" || $1 == "campaign_id" ||
    $1 == "campaign_mode" || $1 == "campaign_label" ||
    $1 == "campaign_order" || $1 == "campaign_pair" ||
    $1 == "campaign_ordinal" || $1 == "campaign_expected_runs" ||
    $1 == "harness_commit" || $1 == "workload_nonce" ||
    $1 == "workload_profile" || $1 == "file_mib" ||
    $1 == "transfer_kib" || $1 == "read_passes" ||
    $1 == "write_passes" || $1 == "queue_depth" ||
    $1 == "post_warmup_settle_seconds" || $1 == "verification_timing" ||
    $1 == "replacement_policy" || $1 == "optional_stopping" {
      if (NF != 2 || $2 == "") exit 1
      seen[$1]++; next
    }
    { exit 1 }
    END {
      split("image vars binary firmware renderer config workload_script quiescence_script quiescence_config environment_policy campaign_registry public_seed schema campaign_id campaign_mode campaign_label campaign_order campaign_pair campaign_ordinal campaign_expected_runs harness_commit workload_nonce workload_profile file_mib transfer_kib read_passes write_passes queue_depth post_warmup_settle_seconds verification_timing replacement_policy optional_stopping", keys, " ")
      if (NR != 32) exit 1
      for (i in keys) if (seen[keys[i]] != 1) exit 1
    }' "$1"
}

nvme_perf_v2_fixed_values() {
  local manifest="$1"
  [[ "$(nvme_perf_v2_value workload_profile "$manifest")" == "$NVME_PERF_V2_PROFILE" \
    && "$(nvme_perf_v2_value file_mib "$manifest")" == "$NVME_PERF_V2_FILE_MIB" \
    && "$(nvme_perf_v2_value transfer_kib "$manifest")" == "$NVME_PERF_V2_TRANSFER_KIB" \
    && "$(nvme_perf_v2_value read_passes "$manifest")" == "$NVME_PERF_V2_READ_PASSES" \
    && "$(nvme_perf_v2_value write_passes "$manifest")" == "$NVME_PERF_V2_WRITE_PASSES" \
    && "$(nvme_perf_v2_value queue_depth "$manifest")" == "$NVME_PERF_V2_QUEUE_DEPTH" \
    && "$(nvme_perf_v2_value post_warmup_settle_seconds "$manifest")" == "$NVME_PERF_V2_SETTLE_SECONDS" \
    && "$(nvme_perf_v2_value verification_timing "$manifest")" == "$NVME_PERF_V2_VERIFICATION_TIMING" ]]
}

nvme_perf_v2_quiescence_config_validate() {
  python3 - "$1" <<'PY'
import json, pathlib, sys
def unique(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("duplicate key")
        result[key] = value
    return result
expected = {
    "schema": "bridgevm.hvf-nvme-quiescence.v1", "samples": 30,
    "interval_seconds": 1,
    "cpu_percent": {"median_max": 10, "p95_max": 20},
    "disk_bytes_per_second": {"median_max": 1048576, "p95_max": 4194304},
    "disk_queue_length": {"p95_max": 0.25},
    "post_ready_settle_seconds": 120, "post_sample_quiet_seconds": 15,
}
try:
    value = json.loads(pathlib.Path(sys.argv[1]).read_bytes().decode("utf-8"),
                       object_pairs_hook=unique)
except (OSError, UnicodeError, json.JSONDecodeError, ValueError):
    raise SystemExit(1)
canonical = lambda item: json.dumps(item, sort_keys=True, separators=(",", ":"))
if canonical(value) != canonical(expected):
    raise SystemExit(1)
PY
}

nvme_perf_v2_renderer_verify() {
  local repo="$1" manifest="$2"
  python3 "$repo/scripts/render-hvf-nvme-workload-v2.py" \
    --verify-output "$(nvme_perf_v2_value workload_script "$manifest")" \
    --verify-config "$(nvme_perf_v2_value config "$manifest")" \
    >/dev/null 2>&1
}

nvme_perf_v2_expected_nonce() {
  printf '%s:%s:%s' "$1" "$2" "$3" | openssl dgst -sha256 -r | cut -c1-32
}

nvme_perf_v2_registry_validate() {
  local registry="$1" seed_path="$2" campaign="$3" harness="$4" binary_hash="$5"
  python3 - "$registry" "$seed_path" "$campaign" "$harness" "$binary_hash" <<'PY'
import hashlib, pathlib, re, sys
registry, seed_path = map(pathlib.Path, sys.argv[1:3])
campaign, harness, binary_hash = sys.argv[3:]
try:
    seed_bytes = seed_path.read_bytes()
    rows = [line.split("\t") for line in registry.read_text(encoding="utf-8").splitlines()]
except (OSError, UnicodeError):
    raise SystemExit(1)
if not re.fullmatch(rb"[0-9a-f]{64}\n", seed_bytes):
    raise SystemExit(1)
seed = seed_bytes[:-1].decode("ascii")
headers = [
    ("schema", "bridgevm.t16-nvme-calibration-registry.v2"),
    ("campaign_id", campaign), ("campaign_mode", "AA"),
    ("workload_profile", "windows-nvme-warm-seq-v2"),
    ("pairs", "24"), ("expected_runs", "48"),
    ("harness_commit", harness), ("public_seed", seed),
    ("public_seed_sha256", hashlib.sha256(seed_bytes).hexdigest()),
    ("binary_a_sha256", binary_hash), ("binary_b_sha256", binary_hash),
    ("order_algorithm", "sha256-balanced-rank-v1"),
    ("replacement_policy", "forbidden"), ("optional_stopping", "forbidden"),
    ("file_mib", "2048"), ("transfer_kib", "128"),
    ("read_passes", "16"), ("write_passes", "4"),
    ("queue_depth", "1"), ("post_warmup_settle_seconds", "30"),
    ("verification_timing", "outside-timed-read"),
]
if len(rows) != len(headers) + 48:
    raise SystemExit(1)
if rows[:len(headers)] != [[key, value] for key, value in headers]:
    raise SystemExit(1)
ranked = sorted((hashlib.sha256(f"{seed}:{pair}".encode()).hexdigest(), pair)
                for pair in range(1, 25))
ab_pairs = {pair for _, pair in ranked[:12]}
expected = []
for pair in range(1, 25):
    order = "AB" if pair in ab_pairs else "BA"
    for index, label in enumerate(order):
        ordinal = (pair - 1) * 2 + index + 1
        expected.append(["lane", str(ordinal), str(pair), order, label, binary_hash,
                         f"t16-{campaign}-{ordinal:03d}"])
if rows[len(headers):] != expected:
    raise SystemExit(1)
PY
}

nvme_perf_v2_verify_hashes() {
  local manifest="$1" key path
  shift
  for key in "$@"; do
    path="$(nvme_perf_v2_value "$key" "$manifest")"
    nvme_perf_v2_regular "$path" \
      && [[ "$(nvme_perf_v2_seal "$path")" == "$(nvme_perf_v2_hash "$key" "$manifest")" ]] \
      || return 1
  done
}

nvme_perf_v2_base_validate() {
  local repo="$1" manifest="$2" expected
  repo="$(cd "$repo" && pwd -P)" || return 1
  nvme_perf_v2_regular "$manifest" && nvme_perf_v2_base_structure "$manifest" || return 1
  [[ "$(nvme_perf_v2_value schema "$manifest")" == "$NVME_PERF_V2_BASE_SCHEMA" ]] || return 1
  nvme_perf_v2_fixed_values "$manifest" || return 1
  [[ "$(nvme_perf_v2_value quiescence_script "$manifest")" == "$repo/scripts/win-assets/bv-nvme-quiescence-v2.ps1" \
    && "$(nvme_perf_v2_value quiescence_config "$manifest")" == "$repo/scripts/live-gates/hvf-nvme-performance-v2-quiescence.json" \
    && "$(nvme_perf_v2_value environment_policy "$manifest")" == "$repo/scripts/live-gates/hvf-nvme-performance-v2-environment.sh" ]] \
    || return 1
  nvme_perf_v2_verify_hashes "$manifest" image vars binary firmware renderer config \
    workload_script quiescence_script quiescence_config environment_policy || return 1
  nvme_perf_v2_quiescence_config_validate "$(nvme_perf_v2_value quiescence_config "$manifest")" \
    && nvme_perf_v2_renderer_verify "$repo" "$manifest"
}

nvme_perf_v2_lane_validate() {
  local repo="$1" manifest="$2" sealed_binary="$3" campaign harness seed_path
  local ordinal pair order label nonce script_nonce expected_label registry
  repo="$(cd "$repo" && pwd -P)" || return 1
  nvme_perf_v2_regular "$manifest" && nvme_perf_v2_lane_structure "$manifest" || return 1
  [[ "$(nvme_perf_v2_value schema "$manifest")" == "$NVME_PERF_V2_LANE_SCHEMA" ]] || return 1
  nvme_perf_v2_fixed_values "$manifest" || return 1
  campaign="$(nvme_perf_v2_value campaign_id "$manifest")"
  harness="$(nvme_perf_v2_value harness_commit "$manifest")"
  ordinal="$(nvme_perf_v2_value campaign_ordinal "$manifest")"
  pair="$(nvme_perf_v2_value campaign_pair "$manifest")"
  order="$(nvme_perf_v2_value campaign_order "$manifest")"
  label="$(nvme_perf_v2_value campaign_label "$manifest")"
  [[ "$campaign" =~ ^[0-9a-f]{32}$ && "$harness" =~ ^[0-9a-f]{40}$ \
    && "$ordinal" =~ ^([1-9]|[1-3][0-9]|4[0-8])$ \
    && "$pair" =~ ^([1-9]|1[0-9]|2[0-4])$ && "$order" =~ ^(AB|BA)$ \
    && "$label" =~ ^[AB]$ && "$(nvme_perf_v2_value campaign_mode "$manifest")" == AA \
    && "$(nvme_perf_v2_value campaign_expected_runs "$manifest")" == "$NVME_PERF_V2_RUNS" \
    && "$(nvme_perf_v2_value replacement_policy "$manifest")" == forbidden \
    && "$(nvme_perf_v2_value optional_stopping "$manifest")" == forbidden ]] || return 1
  (( pair == (ordinal + 1) / 2 )) || return 1
  (( ordinal % 2 == 1 )) && expected_label="${order:0:1}" || expected_label="${order:1:1}"
  [[ "$label" == "$expected_label" ]] || return 1
  seed_path="$(nvme_perf_v2_value public_seed "$manifest")"
  registry="$(nvme_perf_v2_value campaign_registry "$manifest")"
  nvme_perf_v2_verify_hashes "$manifest" image vars binary firmware renderer config \
    workload_script quiescence_script quiescence_config environment_policy campaign_registry public_seed \
    || return 1
  local key repo_path
  for key in quiescence_script quiescence_config environment_policy; do
    case "$key" in
      quiescence_script) repo_path="$repo/scripts/win-assets/bv-nvme-quiescence-v2.ps1" ;;
      quiescence_config) repo_path="$repo/scripts/live-gates/hvf-nvme-performance-v2-quiescence.json" ;;
      environment_policy) repo_path="$repo/scripts/live-gates/hvf-nvme-performance-v2-environment.sh" ;;
    esac
    nvme_perf_v2_regular "$repo_path" \
      && [[ "$(nvme_perf_v2_seal "$repo_path")" == "$(nvme_perf_v2_hash "$key" "$manifest")" ]] \
      || return 1
  done
  nvme_perf_v2_regular "$sealed_binary" \
    && [[ "$(nvme_perf_v2_seal "$sealed_binary")" == "$(nvme_perf_v2_hash binary "$manifest")" ]] \
    || return 1
  nvme_perf_v2_quiescence_config_validate "$(nvme_perf_v2_value quiescence_config "$manifest")" \
    && nvme_perf_v2_renderer_verify "$repo" "$manifest" || return 1
  nonce="$(nvme_perf_v2_expected_nonce "$(tr -d '\n' < "$seed_path")" "$campaign" "$ordinal")"
  [[ "$(nvme_perf_v2_value workload_nonce "$manifest")" == "$nonce" ]] || return 1
  script_nonce="$(LC_ALL=C tr -d '\r' < "$(nvme_perf_v2_value workload_script "$manifest")" \
    | sed -n "s/^\$Nonce = '\([0-9a-f]\{32\}\)'$/\1/p")"
  [[ "$script_nonce" == "$nonce" ]] || return 1
  nvme_perf_v2_registry_validate "$registry" "$seed_path" "$campaign" "$harness" \
    "$(nvme_perf_v2_hash binary "$manifest")"
}

nvme_perf_v2_manifest_self_test() (
  set -euo pipefail
  local repo temporary seed campaign harness binary_hash registry key path base lane order label nonce
  repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
  temporary="$(mktemp -d "${TMPDIR:-/tmp}/bridgevm-nvme-v2-manifest.XXXXXX")"
  temporary="$(cd "$temporary" && pwd -P)"
  trap 'rm -rf "$temporary"' EXIT
  seed="$(printf 'ab%.0s' {1..32})"; printf '%s\n' "$seed" > "$temporary/public-seed.txt"
  printf 'binary\n' > "$temporary/binary"; binary_hash="$(nvme_perf_v2_seal "$temporary/binary")"
  campaign=0123456789abcdef0123456789abcdef
  harness="$(git -C "$repo" rev-parse HEAD)"
  registry="$temporary/registry.tsv"
  printf '%s\n' $'schema\tbridgevm.t16-nvme-calibration-registry.v2' \
    $'campaign_mode\tAA' $'workload_profile\twindows-nvme-warm-seq-v2' \
    $'pairs\t24' $'expected_runs\t48' > "$registry"
  printf 'campaign_id\t%s\n' "$campaign" >> "$registry"
  # Restore the canonical header order after writing its variable fields.
  python3 - "$registry" "$campaign" "$harness" "$seed" \
    "$(nvme_perf_v2_seal "$temporary/public-seed.txt")" "$binary_hash" <<'PY'
import pathlib, sys
path = pathlib.Path(sys.argv[1])
campaign, harness, seed, seed_hash, binary_hash = sys.argv[2:]
lines = [
    "schema\tbridgevm.t16-nvme-calibration-registry.v2",
    f"campaign_id\t{campaign}", "campaign_mode\tAA",
    "workload_profile\twindows-nvme-warm-seq-v2", "pairs\t24", "expected_runs\t48",
    f"harness_commit\t{harness}", f"public_seed\t{seed}",
    f"public_seed_sha256\t{seed_hash}", f"binary_a_sha256\t{binary_hash}",
    f"binary_b_sha256\t{binary_hash}", "order_algorithm\tsha256-balanced-rank-v1",
    "replacement_policy\tforbidden", "optional_stopping\tforbidden",
    "file_mib\t2048", "transfer_kib\t128", "read_passes\t16", "write_passes\t4",
    "queue_depth\t1", "post_warmup_settle_seconds\t30",
    "verification_timing\toutside-timed-read",
]
path.write_text("\n".join(lines) + "\n", encoding="utf-8")
PY
  python3 - "$registry" "$seed" "$campaign" "$binary_hash" <<'PY'
import hashlib, pathlib, sys
path = pathlib.Path(sys.argv[1])
seed, campaign, binary_hash = sys.argv[2:]
ranked = sorted((hashlib.sha256(f"{seed}:{p}".encode()).hexdigest(), p) for p in range(1, 25))
ab = {p for _, p in ranked[:12]}
with path.open("a", encoding="utf-8", newline="") as stream:
    for pair in range(1, 25):
        order = "AB" if pair in ab else "BA"
        for index, label in enumerate(order):
            ordinal = (pair - 1) * 2 + index + 1
            stream.write(f"lane\t{ordinal}\t{pair}\t{order}\t{label}\t{binary_hash}\tt16-{campaign}-{ordinal:03d}\n")
PY
  nvme_perf_v2_registry_validate "$registry" "$temporary/public-seed.txt" "$campaign" "$harness" "$binary_hash"
  for key in image vars firmware renderer; do printf 'fixture-%s\n' "$key" > "$temporary/$key"; done
  nonce="$(nvme_perf_v2_expected_nonce "$seed" "$campaign" 1)"
  python3 "$repo/scripts/render-hvf-nvme-workload-v2.py" \
    --output "$temporary/workload.ps1" --config-output "$temporary/config.json" \
    --nonce "$nonce" >/dev/null
  base="$temporary/base.tsv"; : > "$base"
  for key in image vars binary firmware renderer; do
    path="$temporary/$key"
    printf '%s\t%s\t%s\n' "$key" "$path" "$(nvme_perf_v2_seal "$path")" >> "$base"
  done
  for key in config workload_script quiescence_script quiescence_config environment_policy; do
    case "$key" in
      config) path="$temporary/config.json" ;;
      workload_script) path="$temporary/workload.ps1" ;;
      quiescence_script) path="$repo/scripts/win-assets/bv-nvme-quiescence-v2.ps1" ;;
      quiescence_config) path="$repo/scripts/live-gates/hvf-nvme-performance-v2-quiescence.json" ;;
      environment_policy) path="$repo/scripts/live-gates/hvf-nvme-performance-v2-environment.sh" ;;
    esac
    printf '%s\t%s\t%s\n' "$key" "$path" "$(nvme_perf_v2_seal "$path")" >> "$base"
  done
  printf '%s\n' $'schema\tbridgevm.t16-nvme-calibration-base.v2' \
    $'workload_profile\twindows-nvme-warm-seq-v2' $'file_mib\t2048' \
    $'transfer_kib\t128' $'read_passes\t16' $'write_passes\t4' $'queue_depth\t1' \
    $'post_warmup_settle_seconds\t30' $'verification_timing\toutside-timed-read' >> "$base"
  nvme_perf_v2_base_validate "$repo" "$base"
  cp "$repo/scripts/win-assets/bv-nvme-quiescence-v2.ps1" "$temporary/bv-nvme-quiescence-v2.ps1"
  cp "$repo/scripts/live-gates/hvf-nvme-performance-v2-quiescence.json" "$temporary/quiescence.json"
  cp "$repo/scripts/live-gates/hvf-nvme-performance-v2-environment.sh" "$temporary/environment.sh"
  lane="$temporary/lane.tsv"; : > "$lane"
  for key in image vars binary firmware renderer config workload_script; do
    case "$key" in
      config) path="$temporary/config.json" ;;
      workload_script) path="$temporary/workload.ps1" ;;
      *) path="$temporary/$key" ;;
    esac
    printf '%s\t%s\t%s\n' "$key" "$path" "$(nvme_perf_v2_seal "$path")" >> "$lane"
  done
  for key in quiescence_script quiescence_config environment_policy campaign_registry public_seed; do
    case "$key" in
      quiescence_script) path="$temporary/bv-nvme-quiescence-v2.ps1" ;;
      quiescence_config) path="$temporary/quiescence.json" ;;
      environment_policy) path="$temporary/environment.sh" ;;
      campaign_registry) path="$registry" ;;
      public_seed) path="$temporary/public-seed.txt" ;;
    esac
    printf '%s\t%s\t%s\n' "$key" "$path" "$(nvme_perf_v2_seal "$path")" >> "$lane"
  done
  order="$(awk -F '\t' '$1 == "lane" && $2 == 1 { print $4 }' "$registry")"
  label="$(awk -F '\t' '$1 == "lane" && $2 == 1 { print $5 }' "$registry")"
  printf '%s\n' $'schema\tbridgevm.t16-nvme-calibration-lane.v2' $'campaign_mode\tAA' \
    $'campaign_expected_runs\t48' $'workload_profile\twindows-nvme-warm-seq-v2' \
    $'file_mib\t2048' $'transfer_kib\t128' $'read_passes\t16' $'write_passes\t4' \
    $'queue_depth\t1' $'post_warmup_settle_seconds\t30' \
    $'verification_timing\toutside-timed-read' $'replacement_policy\tforbidden' \
    $'optional_stopping\tforbidden' >> "$lane"
  printf 'campaign_id\t%s\ncampaign_label\t%s\ncampaign_order\t%s\n' \
    "$campaign" "$label" "$order" >> "$lane"
  printf 'campaign_pair\t1\ncampaign_ordinal\t1\nharness_commit\t%s\nworkload_nonce\t%s\n' \
    "$harness" "$nonce" >> "$lane"
  nvme_perf_v2_lane_validate "$repo" "$lane" "$temporary/binary"
  "$repo/scripts/live-gates/run-hvf-nvme-performance-tier.sh" \
    --out "$temporary/validate-only-out" --input-manifest "$lane" \
    --sealed-binary "$temporary/binary" --validate-only | grep -q PASS
  [[ ! -e "$temporary/validate-only-out" ]]
  cp "$lane" "$temporary/unknown-row.tsv"; printf 'unknown\tvalue\n' >> "$temporary/unknown-row.tsv"
  ! nvme_perf_v2_lane_validate "$repo" "$temporary/unknown-row.tsv" "$temporary/binary"
  printf 'mutated\n' >> "$temporary/config.json"
  ! nvme_perf_v2_base_validate "$repo" "$base"
  cp "$registry" "$temporary/bad-registry.tsv"
  sed -i.bak $'s/binary_b_sha256\t[0-9a-f]*/binary_b_sha256\tffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff/' "$temporary/bad-registry.tsv"
  ! nvme_perf_v2_registry_validate "$temporary/bad-registry.tsv" "$temporary/public-seed.txt" "$campaign" "$harness" "$binary_hash"
  printf '%s' "$seed" > "$temporary/public-seed.txt"
  ! nvme_perf_v2_registry_validate "$registry" "$temporary/public-seed.txt" "$campaign" "$harness" "$binary_hash"
  printf '%s\n' 'HVF NVMe calibration v2 manifest self-test: PASS'
)

nvme_perf_v2_manifest_main() {
  case "${1:-}" in
    validate-base)
      [[ $# -eq 3 ]] || return 2
      nvme_perf_v2_base_validate "$2" "$3" \
        && printf '%s\n' 'HVF NVMe calibration v2 base manifest: valid'
      ;;
    validate-lane)
      [[ $# -eq 4 ]] || return 2
      nvme_perf_v2_lane_validate "$2" "$3" "$4" \
        && printf '%s\n' 'HVF NVMe calibration v2 lane manifest: valid'
      ;;
    self-test) [[ $# -eq 1 ]] && nvme_perf_v2_manifest_self_test ;;
    *)
      printf 'usage: %s {validate-base REPO MANIFEST|validate-lane REPO MANIFEST SEALED_BINARY|self-test}\n' "$0" >&2
      return 2
      ;;
  esac
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  set -euo pipefail
  nvme_perf_v2_manifest_main "$@"
fi
