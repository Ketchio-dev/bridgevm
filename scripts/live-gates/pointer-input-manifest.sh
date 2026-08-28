#!/usr/bin/env bash
# Verify the exact private inputs consumed by the B4 t8 tier.
set -euo pipefail

pointer_tree_hash() {
  [[ -d "$1" && ! -L "$1" ]] || { printf absent; return; }
  (cd "$1" && find . -type f ! -type l -exec shasum -a 256 {} + \
    | LC_ALL=C sort -k2 | shasum -a 256 | cut -d' ' -f1 | tr -d '\n')
}

pointer_manifest_path() {
  awk -F '\t' -v key="$1" '$1 == key { print $2 }' "$INPUT_MANIFEST"
}

verify_pointer_manifest() {
  [[ -f "$INPUT_MANIFEST" && ! -L "$INPUT_MANIFEST" ]] || return 1
  awk -F '\t' '
    BEGIN { split("image vars viogpu_dir", required, " ") }
    NF != 3 || substr($2,1,1) != "/" || $3 !~ /^[0-9a-f]{64}$/ { bad=1 }
    { seen[$1]++ }
    END {
      for (i in required) if (seen[required[i]] != 1) bad=1
      for (key in seen) {
        allowed=0; for (i in required) if (key == required[i]) allowed=1
        if (!allowed || seen[key] != 1) bad=1
      }
      exit bad
    }
  ' "$INPUT_MANIFEST" || return 1
  : > "$OUT/verified-inputs.tsv"
  local key path expected actual
  while IFS=$'\t' read -r key path expected; do
    if [[ "$key" == viogpu_dir ]]; then actual="$(pointer_tree_hash "$path")"
    else [[ -f "$path" && ! -L "$path" ]] || actual=absent; [[ "$actual" == absent ]] || actual="$(seal "$path")"; fi
    [[ "$actual" == "$expected" ]] || { echo "B4 input hash mismatch: $key" >&2; return 1; }
    printf '%s\t%s\n' "$key" "$actual" >> "$OUT/verified-inputs.tsv"
  done < "$INPUT_MANIFEST"
}
