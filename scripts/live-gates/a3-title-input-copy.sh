#!/usr/bin/env bash
# Copy already-verified A3 file inputs and recheck bytes before use.
set -euo pipefail
a3_copy_verified_file() {
    local key="$1" destination="$2" expected
    expected="$(awk -F '\t' -v key="$key" '$1 == key { print $2 }' \
        "$OUT/verified-inputs.tsv")"
    cp "$(a3_manifest_path "$key")" "$destination" \
        && [[ "$(seal "$destination")" == "$expected" ]]
}

a3_materialize_title_inputs() {
    ppsspp="$OUT/ppsspp-payload.zip"
    d3d11="$OUT/d3d11.dll"
    dxgi="$OUT/dxgi.dll"
    a3_copy_verified_file ppsspp "$ppsspp" \
        && a3_copy_verified_file d3d11 "$d3d11" \
        && a3_copy_verified_file dxgi "$dxgi"
}
