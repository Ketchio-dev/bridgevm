#!/usr/bin/env bash

perf_seal() {
  openssl dgst -sha256 -r "$1" 2>/dev/null | cut -d' ' -f1 | tr -d '\n'
}

perf_manifest_value() {
  awk -F '\t' -v key="$1" '$1 == key { print $2; exit }' "$2"
}

perf_manifest_hash() {
  awk -F '\t' -v key="$1" '$1 == key { print $3; exit }' "$2"
}

perf_manifest_validate() {
  local repo="$1" manifest="$2" sealed_binary="$3"
  [[ -f "$manifest" && "$sealed_binary" == /* && -f "$sealed_binary" && ! -L "$sealed_binary" ]] || return 1
  awk -F '\t' '
    $1 == "image" || $1 == "vars" || $1 == "binary" || $1 == "renderer" {
      if (NF != 3 || $2 !~ /^\// || $3 !~ /^[0-9a-f]{64}$/) exit 1
      seen[$1]++; next
    }
    $1 == "binary_source_commit" || $1 == "binary_profile" ||
    $1 == "binary_features" || $1 == "rust_toolchain" ||
    $1 == "campaign_id" || $1 == "campaign_mode" ||
    $1 == "campaign_role" || $1 == "campaign_ordinal" ||
    $1 == "campaign_expected_runs" {
      if (NF != 2 || $2 == "") exit 1
      seen[$1]++; next
    }
    { exit 1 }
    END {
      split("image vars binary renderer binary_source_commit binary_profile binary_features rust_toolchain campaign_id campaign_mode campaign_role campaign_ordinal campaign_expected_runs", keys, " ")
      if (NR != 13) exit 1
      for (i in keys) if (seen[keys[i]] != 1) exit 1
    }' "$manifest" || return 1
  local image vars renderer ordinal expected role mode
  image="$(perf_manifest_value image "$manifest")"; vars="$(perf_manifest_value vars "$manifest")"; renderer="$(perf_manifest_value renderer "$manifest")"
  [[ -f "$image" && ! -L "$image" && -f "$vars" && ! -L "$vars" && -f "$renderer" && ! -L "$renderer" ]] || return 1
  PERF_BINARY_SOURCE_COMMIT="$(perf_manifest_value binary_source_commit "$manifest")"
  PERF_BINARY_PROFILE="$(perf_manifest_value binary_profile "$manifest")"
  PERF_BINARY_FEATURES="$(perf_manifest_value binary_features "$manifest")"
  PERF_RUST_TOOLCHAIN="$(perf_manifest_value rust_toolchain "$manifest")"
  PERF_CAMPAIGN_ID="$(perf_manifest_value campaign_id "$manifest")"
  mode="$(perf_manifest_value campaign_mode "$manifest")"; role="$(perf_manifest_value campaign_role "$manifest")"
  ordinal="$(perf_manifest_value campaign_ordinal "$manifest")"; expected="$(perf_manifest_value campaign_expected_runs "$manifest")"
  [[ "$PERF_BINARY_SOURCE_COMMIT" =~ ^[0-9a-f]{40}$ && "$PERF_BINARY_PROFILE" == release \
    && "$PERF_BINARY_FEATURES" =~ ^[a-z0-9,+_-]+$ && "$PERF_RUST_TOOLCHAIN" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ \
    && "$PERF_CAMPAIGN_ID" =~ ^[0-9a-f]{32}$ && "$mode" =~ ^A[AB]$ \
    && "$role" =~ ^(baseline|candidate)$ && "$ordinal" =~ ^[0-9]+$ && "$expected" =~ ^[0-9]+$ ]] || return 1
  (( expected >= 6 && expected % 2 == 0 && ordinal >= 1 && ordinal <= expected )) || return 1
  { (( ordinal % 2 == 1 )) && [[ "$role" == baseline ]]; } || { (( ordinal % 2 == 0 )) && [[ "$role" == candidate ]]; } || return 1
  git -C "$repo" cat-file -e "$PERF_BINARY_SOURCE_COMMIT^{commit}" 2>/dev/null || return 1
  PERF_CAMPAIGN_MODE="$mode"; PERF_CAMPAIGN_ROLE="$role"; PERF_CAMPAIGN_ORDINAL="$ordinal"; PERF_CAMPAIGN_EXPECTED_RUNS="$expected"
  PERF_BINARY_HASH="$(perf_seal "$sealed_binary")"
  [[ "$PERF_BINARY_HASH" == "$(perf_manifest_hash binary "$manifest")" ]]
}

perf_manifest_verify_source_hashes() {
  local manifest="$1" image vars renderer
  image="$(perf_manifest_value image "$manifest")"; vars="$(perf_manifest_value vars "$manifest")"; renderer="$(perf_manifest_value renderer "$manifest")"
  [[ "$(perf_seal "$image")" == "$(perf_manifest_hash image "$manifest")" \
    && "$(perf_seal "$vars")" == "$(perf_manifest_hash vars "$manifest")" && "$(perf_seal "$renderer")" == "$(perf_manifest_hash renderer "$manifest")" ]]
}
