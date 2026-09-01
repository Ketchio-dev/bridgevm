#!/usr/bin/env bash

# Strict parser for one sealed T16 NVMe performance input manifest.

nvme_perf_seal() {
  openssl dgst -sha256 -r "$1" 2>/dev/null | cut -d' ' -f1 | tr -d '\n'
}

nvme_perf_manifest_value() {
  LC_ALL=C awk -F '\t' -v key="$1" '$1 == key { print $2; exit }' "$2"
}

nvme_perf_manifest_hash() {
  LC_ALL=C awk -F '\t' -v key="$1" '$1 == key { print $3; exit }' "$2"
}

nvme_perf_expected_role() {
  local ordinal="$1"
  if (( ((((ordinal - 1) / 2) + ordinal) % 2) == 1 )); then
    printf '%s\n' baseline
  else
    printf '%s\n' candidate
  fi
}

nvme_perf_manifest_validate() {
  local repo="$1" manifest="$2" sealed_binary="$3"
  [[ -f "$manifest" && ! -L "$manifest" && "$sealed_binary" == /* \
    && -f "$sealed_binary" && ! -L "$sealed_binary" ]] || return 1
  git -C "$repo" rev-parse --git-dir >/dev/null 2>&1 || return 1
  LC_ALL=C awk -F '\t' '
    $1 == "image" || $1 == "vars" || $1 == "binary" ||
    $1 == "firmware" || $1 == "renderer" || $1 == "config" ||
    $1 == "workload_script" || $1 == "campaign_registry" {
      if (NF != 3 || $2 !~ /^\// || $3 !~ /^[0-9a-f]{64}$/) exit 1
      seen[$1]++; next
    }
    $1 == "campaign_id" || $1 == "campaign_mode" ||
    $1 == "campaign_role" || $1 == "campaign_ordinal" ||
    $1 == "campaign_expected_runs" || $1 == "workload_profile" ||
    $1 == "file_mib" || $1 == "transfer_kib" ||
    $1 == "read_passes" || $1 == "write_passes" {
      if (NF != 2 || $2 == "") exit 1
      seen[$1]++; next
    }
    { exit 1 }
    END {
      split("image vars binary firmware renderer config workload_script campaign_registry campaign_id campaign_mode campaign_role campaign_ordinal campaign_expected_runs workload_profile file_mib transfer_kib read_passes write_passes", keys, " ")
      if (NR != 18) exit 1
      for (i in keys) if (seen[keys[i]] != 1) exit 1
    }' "$manifest" || return 1

  local key path mode role ordinal expected nonce expected_nonce
  for key in image vars firmware renderer config workload_script campaign_registry; do
    path="$(nvme_perf_manifest_value "$key" "$manifest")"
    [[ -f "$path" && ! -L "$path" ]] || return 1
  done

  mode="$(nvme_perf_manifest_value campaign_mode "$manifest")"
  role="$(nvme_perf_manifest_value campaign_role "$manifest")"
  ordinal="$(nvme_perf_manifest_value campaign_ordinal "$manifest")"
  expected="$(nvme_perf_manifest_value campaign_expected_runs "$manifest")"
  [[ "$(nvme_perf_manifest_value campaign_id "$manifest")" =~ ^[0-9a-f]{32}$ \
    && "$mode" =~ ^A[AB]$ && "$role" =~ ^(baseline|candidate)$ \
    && "$ordinal" =~ ^[1-9][0-9]*$ && "$expected" =~ ^[1-9][0-9]*$ \
    && "$(nvme_perf_manifest_value workload_profile "$manifest")" == windows-nvme-warm-seq-v1 \
    && "$(nvme_perf_manifest_value file_mib "$manifest")" == 512 \
    && "$(nvme_perf_manifest_value transfer_kib "$manifest")" == 128 \
    && "$(nvme_perf_manifest_value read_passes "$manifest")" == 5 \
    && "$(nvme_perf_manifest_value write_passes "$manifest")" == 2 ]] || return 1
  (( expected >= 20 && expected <= 200 && expected % 4 == 0 && ordinal >= 1 && ordinal <= expected )) || return 1
  [[ "$role" == "$(nvme_perf_expected_role "$ordinal")" ]] || return 1
  nonce="$(LC_ALL=C tr -d '\r' < "$(nvme_perf_manifest_value workload_script "$manifest")" \
    | sed -n "s/^\$Nonce = '\([0-9a-f]\{32\}\)'$/\1/p")"
  expected_nonce="$(printf '%s:%s' "$(nvme_perf_manifest_value campaign_id "$manifest")" "$ordinal" \
    | openssl dgst -sha256 -r | cut -c1-32)"
  [[ "$nonce" == "$expected_nonce" ]] || return 1
  python3 "$repo/scripts/render-hvf-nvme-workload.py" \
    --verify-output "$(nvme_perf_manifest_value workload_script "$manifest")" \
    --verify-config "$(nvme_perf_manifest_value config "$manifest")" \
    --file-mib 512 --transfer-kib 128 --read-passes 5 --write-passes 2 \
    >/dev/null 2>&1 || return 1
  [[ "$(nvme_perf_seal "$sealed_binary")" == "$(nvme_perf_manifest_hash binary "$manifest")" ]] || return 1

  NVME_PERF_CAMPAIGN_ID="$(nvme_perf_manifest_value campaign_id "$manifest")"
  NVME_PERF_CAMPAIGN_MODE="$mode"
  NVME_PERF_CAMPAIGN_ROLE="$role"
  NVME_PERF_CAMPAIGN_ORDINAL="$ordinal"
  NVME_PERF_CAMPAIGN_EXPECTED_RUNS="$expected"
  NVME_PERF_BINARY_HASH="$(nvme_perf_seal "$sealed_binary")"
}

nvme_perf_manifest_verify_source_hashes() {
  local manifest="$1" key path
  for key in image vars binary firmware renderer config workload_script campaign_registry; do
    path="$(nvme_perf_manifest_value "$key" "$manifest")"
    [[ -f "$path" && ! -L "$path" \
      && "$(nvme_perf_seal "$path")" == "$(nvme_perf_manifest_hash "$key" "$manifest")" ]] || return 1
  done
}

nvme_perf_manifest_self_test() {
  local repo temporary manifest key path hash nonce
  repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
  temporary="$(mktemp -d "${TMPDIR:-/tmp}/bridgevm-nvme-manifest.XXXXXX")" || return 1
  trap "rm -rf '$temporary'" RETURN
  for key in image vars binary firmware renderer; do
    path="$temporary/$key"
    printf 'fixture-%s\n' "$key" > "$path"
  done
  nonce="$(printf '%s' '00000000000000000000000000000000:1' | openssl dgst -sha256 -r | cut -c1-32)"
  python3 "$repo/scripts/render-hvf-nvme-workload.py" \
    --output "$temporary/workload_script" --config-output "$temporary/config" \
    --nonce "$nonce" >/dev/null || return 1
  printf 'schema\tbridgevm.t16-campaign-registry.v1\ncampaign_id\t00000000000000000000000000000000\ncampaign_mode\tAA\npairs\t10\nexpected_runs\t20\nharness_commit\t%s\nlane\t1\tbaseline\tt16-00000000000000000000000000000000-001\n' \
    "$(git -C "$repo" rev-parse HEAD)" > "$temporary/campaign_registry"
  manifest="$temporary/input-manifest.tsv"
  for key in image vars binary firmware renderer config workload_script campaign_registry; do
    path="$temporary/$key"; hash="$(nvme_perf_seal "$path")"
    printf '%s\t%s\t%s\n' "$key" "$path" "$hash" >> "$manifest"
  done
  printf '%s\n' \
    $'campaign_id\t00000000000000000000000000000000' $'campaign_mode\tAA' \
    $'campaign_role\tbaseline' $'campaign_ordinal\t1' $'campaign_expected_runs\t20' \
    $'workload_profile\twindows-nvme-warm-seq-v1' $'file_mib\t512' \
    $'transfer_kib\t128' $'read_passes\t5' $'write_passes\t2' >> "$manifest"
  cp "$temporary/binary" "$temporary/sealed_binary"
  nvme_perf_manifest_validate "$repo" "$manifest" "$temporary/sealed_binary" || return 1
  nvme_perf_manifest_verify_source_hashes "$manifest" || return 1
  rm "$temporary/binary"
  nvme_perf_manifest_validate "$repo" "$manifest" "$temporary/sealed_binary" || return 1
  ! nvme_perf_manifest_verify_source_hashes "$manifest" || return 1
  cp "$temporary/sealed_binary" "$temporary/binary"
  cp "$manifest" "$temporary/bad.tsv"
  sed -i.bak $'s/read_passes\t5/read_passes\t4/' "$temporary/bad.tsv"
  ! nvme_perf_manifest_validate "$repo" "$temporary/bad.tsv" "$temporary/sealed_binary" || return 1
  printf 'tamper\n' >> "$temporary/config"
  ! nvme_perf_manifest_verify_source_hashes "$manifest" || return 1
  printf '%s\n' "HVF NVMe performance manifest self-test: PASS"
}

nvme_perf_manifest_main() {
  case "${1:-}" in
    validate)
      [[ $# -eq 4 ]] || { printf 'usage: %s validate REPO MANIFEST SEALED_BINARY\n' "$0" >&2; return 2; }
      nvme_perf_manifest_validate "$2" "$3" "$4" \
        && nvme_perf_manifest_verify_source_hashes "$3" \
        && printf '%s\n' "HVF NVMe performance manifest: valid"
      ;;
    self-test) [[ $# -eq 1 ]] || return 2; nvme_perf_manifest_self_test ;;
    *) printf 'usage: %s {validate REPO MANIFEST SEALED_BINARY|self-test}\n' "$0" >&2; return 2 ;;
  esac
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  set -euo pipefail
  nvme_perf_manifest_main "$@"
fi
