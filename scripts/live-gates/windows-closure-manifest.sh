#!/usr/bin/env bash
# Strict verification for the exact external inputs to the Windows 1.0 closure tier.
set -euo pipefail

seal_closure_tree() {
  [[ -d "$1" && ! -L "$1" ]] || { printf absent; return; }
  (cd "$1" && find . -type f ! -type l -exec shasum -a 256 {} + \
    | LC_ALL=C sort -k2 | shasum -a 256 | cut -d' ' -f1 | tr -d '\n')
}

closure_manifest_path() {
  awk -F '\t' -v key="$1" '$1 == key { print $2 }' "$INPUT_MANIFEST"
}

verify_closure_manifest() {
  [[ -f "$INPUT_MANIFEST" && ! -L "$INPUT_MANIFEST" ]] || return 1
  awk -F '\t' '
    BEGIN {
      split("image vars injector injector_assets agent viogpu_dir virglrenderer moltenvk binary", required, " ")
    }
    NF != 3 || substr($2, 1, 1) != "/" || length($3) != 64 || $3 !~ /^[0-9a-f]+$/ { bad = 1 }
    { seen[$1]++ }
    END {
      for (i in required) if (seen[required[i]] != 1) bad = 1
      for (key in seen) {
        allowed = 0
        for (i in required) if (key == required[i]) allowed = 1
        if (!allowed || seen[key] != 1) bad = 1
      }
      exit bad
    }
  ' "$INPUT_MANIFEST" || return 1

  : > "$OUT/verified-inputs.tsv"
  local key path expected actual
  while IFS=$'\t' read -r key path expected; do
    actual=""; if [[ "$key" == viogpu_dir ]]; then
      actual="$(seal_closure_tree "$path")"
    elif [[ "$key" == binary ]]; then
      actual="$(seal "$SEALED_BINARY")"
    else
      [[ -f "$path" && ! -L "$path" ]] || actual=absent
      [[ -n "${actual:-}" ]] || actual="$(seal "$path")"
    fi
    [[ "$actual" == "$expected" ]] || {
      echo "Windows closure input hash mismatch: $key" >&2
      return 1
    }
    printf '%s\t%s\n' "$key" "$actual" >> "$OUT/verified-inputs.tsv"
  done < "$INPUT_MANIFEST"

  local assets_path agent_path expected_assets expected_agent embedded_assets embedded_agent
  assets_path="$(closure_manifest_path injector_assets)"
  agent_path="$(closure_manifest_path agent)"
  expected_assets="$(awk -F '\t' '$1 == "injector_assets" { print $3 }' "$INPUT_MANIFEST")"
  expected_agent="$(awk -F '\t' '$1 == "agent" { print $3 }' "$INPUT_MANIFEST")"
  embedded_assets="$(tr -d '\r\n' < "$assets_path")"
  [[ "$embedded_assets" =~ ^[0-9a-f]{64}$ ]] || return 1
  embedded_agent="$(shasum -a 256 "$agent_path" | cut -d' ' -f1)"
  [[ "$(seal "$assets_path")" == "$expected_assets" && "$embedded_agent" == "$expected_agent" ]] || return 1
  printf 'injector_assets_digest\t%s\nagent_sha256\t%s\n' "$embedded_assets" "$embedded_agent" \
    >> "$OUT/verified-inputs.tsv"
}
