#!/usr/bin/env bash
# Run the release A3 campaign from one sealed input manifest.
set -euo pipefail
REPO="$(cd "$(dirname "$0")/../.." && pwd)"
OUT=""
INPUT_MANIFEST=""
SEALED_BINARY=""
JOB_ID="local-a3-$(date +%Y%m%d-%H%M%S)"
while [ $# -gt 0 ]; do
    case "$1" in
        --out) OUT="$2"; shift 2 ;;
        --input-manifest) INPUT_MANIFEST="$2"; shift 2 ;;
        --sealed-binary) SEALED_BINARY="$2"; shift 2 ;;
        --job-id) JOB_ID="$2"; shift 2 ;;
        *) echo "unknown A3 tier option $1" >&2; exit 2 ;;
    esac
done
[ -n "$OUT" ] || { echo "A3 tier needs --out" >&2; exit 2; }
mkdir -p "$OUT"
seal() {
    [[ -r "$1" ]] || { printf 'absent'; return; }
    openssl dgst -sha256 -r "$1" 2>/dev/null | cut -d' ' -f1 | tr -d '\n'
}
seal_tree() {
    [[ -d "$1" ]] || { printf 'absent'; return; }
    (cd "$1" && find . -type f -exec shasum -a 256 {} + \
        | LC_ALL=C sort -k2 | shasum -a 256 | cut -d' ' -f1 | tr -d '\n')
}
manifest_path() {
    awk -F '\t' -v key="$1" '$1 == key { print $2 }' "$INPUT_MANIFEST"
}
write_refusal() {
    python3 "$REPO/scripts/live-gates/write-a3-title-receipt.py" \
        --out "$OUT" --job-id "$JOB_ID" \
        --commit "$(git -C "$REPO" rev-parse HEAD)" --reason "$1"
}
on_exit() {
    local status="$?"
    [[ "$status" -eq 0 || -f "$OUT/receipt.json" ]] || write_refusal failed-before-receipt || true
    return "$status"
}
trap on_exit EXIT
verify_manifest() {
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
            actual="$(seal_tree "$path")"
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
if ! verify_manifest; then
    echo "A3 input manifest is missing, malformed, or does not match the live inputs" >&2
    write_refusal refused-input-mismatch
    exit 1
fi
image="$(manifest_path image)"
vars="$(manifest_path vars)"
title="$(manifest_path title)"
ppsspp="$(manifest_path ppsspp)"
d3d11="$(manifest_path d3d11)"
viogpu_dir="$(manifest_path viogpu_dir)"
virglrenderer="$(manifest_path virglrenderer)"
moltenvk="$(manifest_path moltenvk)"
export BRIDGEVM_VENUS_PREFIX="$(dirname "$(dirname "$virglrenderer")")"
export BRIDGEVM_VULKAN_LIB="$moltenvk"
cd "$REPO"
binary_hash="$(seal "$SEALED_BINARY")"
if ! codesign --verify --strict "$SEALED_BINARY" \
    || ! otool -L "$SEALED_BINARY" | grep -Fq "$virglrenderer"; then
    echo "A3 binary is not linked to the sealed virglrenderer" >&2
    write_refusal refused-renderer-mismatch
    exit 1
fi
gate_asset_hash="$({
    shasum -a 256 scripts/verify-d3d11-title-fps.sh
    shasum -a 256 scripts/win-assets/bvgpu-real-title-gate.ps1
    shasum -a 256 scripts/win-assets/bvgpu-d3d11-identity.ps1
    shasum -a 256 scripts/win-assets/bv-ppsspp-d3d11.ini
} | shasum -a 256 | cut -d' ' -f1)"

started_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
for run in 1 2 3; do
    run_out="$OUT/run-$run"
    TARGET="$image" VARS="$vars" OUT="$run_out" \
        TITLE_ISO="$title" PPSSPP_EXECUTABLE="$ppsspp" \
        D3D11_DLL="$d3d11" DXGI_DLL="$(manifest_path dxgi)" \
        VIOGPU3D_DIR="$viogpu_dir" PRESTAGED_TITLE=1 SKIP_BUILD=1 \
        BRIDGEVM_PREBUILT_PROBE="$SEALED_BINARY" \
        "$REPO/scripts/verify-d3d11-title-fps.sh" || true
done

python3 "$REPO/scripts/live-gates/write-a3-title-receipt.py" \
    --out "$OUT" --job-id "$JOB_ID" \
    --commit "$(git -C "$REPO" rev-parse HEAD)" \
    --started-at "$started_at" --binary-hash "$binary_hash" \
    --gate-asset-hash "$gate_asset_hash" \
    --input-manifest-hash "$(seal "$INPUT_MANIFEST")"
