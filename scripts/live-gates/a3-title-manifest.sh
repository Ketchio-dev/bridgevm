#!/usr/bin/env bash
# Shared strict verification for the ten-input A3 title manifest.
set -euo pipefail

seal_a3_tree() {
    [[ -d "$1" ]] || { printf absent; return; }
    (cd "$1" && find . -type f -exec shasum -a 256 {} + \
        | LC_ALL=C sort -k2 | shasum -a 256 | cut -d' ' -f1 | tr -d '\n')
}

a3_manifest_path() {
    awk -F '\t' -v key="$1" '$1 == key { print $2 }' "$INPUT_MANIFEST"
}

verify_a3_manifest() {
    [[ -f "$INPUT_MANIFEST" ]] || return 1
    awk -F '\t' '
        BEGIN { split("image vars title ppsspp d3d11 dxgi viogpu_dir virglrenderer moltenvk binary", required, " ") }
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
        if [[ "$key" == viogpu_dir ]]; then
            actual="$(seal_a3_tree "$path")"
        elif [[ "$key" == binary ]]; then
            actual="$(seal "$SEALED_BINARY")"
        else
            actual="$(seal "$path")"
        fi
        [[ "$actual" == "$expected" ]] || {
            echo "A3 input hash mismatch: $key" >&2
            return 1
        }
        printf '%s\t%s\n' "$key" "$actual" >> "$OUT/verified-inputs.tsv"
    done < "$INPUT_MANIFEST"
}
