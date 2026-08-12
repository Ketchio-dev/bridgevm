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
source "$REPO/scripts/live-gates/a3-title-payload.sh"
source "$REPO/scripts/live-gates/a3-title-manifest.sh"; source "$REPO/scripts/live-gates/a3-title-input-copy.sh"
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
if ! verify_a3_manifest; then
    echo "A3 input manifest is missing, malformed, or does not match the live inputs" >&2
    write_refusal refused-input-mismatch
    exit 1
fi
image="$(a3_manifest_path image)"
vars="$(a3_manifest_path vars)"
title="$(a3_manifest_path title)"
a3_materialize_title_inputs || { write_refusal refused-input-copy-mismatch; exit 1; }
ppsspp_executable_hash="$(a3_validate_ppsspp_payload "$ppsspp")" \
    || { write_refusal refused-ppsspp-payload; exit 1; }
viogpu_dir="$(a3_manifest_path viogpu_dir)"
virglrenderer="$(a3_manifest_path virglrenderer)"
moltenvk="$(a3_manifest_path moltenvk)"
export BRIDGEVM_VENUS_PREFIX="$(dirname "$(dirname "$virglrenderer")")" \
    BRIDGEVM_VULKAN_LIB="$moltenvk" MVK_CONFIG_LOG_LEVEL="${MVK_CONFIG_LOG_LEVEL:-3}"
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
    shasum -a 256 scripts/win-assets/bvgpu-stage-ppsspp.ps1
    shasum -a 256 scripts/live-gates/a3-title-payload.{sh,py}
    shasum -a 256 scripts/live-gates/a3-title-payload-stage.sh
    shasum -a 256 scripts/live-gates/a3-title-input-copy.sh
} | shasum -a 256 | cut -d' ' -f1)"

started_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
for run in 1 2 3; do
    run_out="$OUT/run-$run"
    TARGET="$image" VARS="$vars" OUT="$run_out" \
        TITLE_ISO="$title" PPSSPP_PAYLOAD="$ppsspp" \
        EXPECTED_PPSSPP_SHA="$ppsspp_executable_hash" \
        D3D11_DLL="$d3d11" DXGI_DLL="$dxgi" \
        VIOGPU3D_DIR="$viogpu_dir" SKIP_BUILD=1 \
        BRIDGEVM_PREBUILT_PROBE="$SEALED_BINARY" \
        "$REPO/scripts/verify-d3d11-title-fps.sh" || break
done

python3 "$REPO/scripts/live-gates/write-a3-title-receipt.py" \
    --out "$OUT" --job-id "$JOB_ID" \
    --commit "$(git -C "$REPO" rev-parse HEAD)" \
    --started-at "$started_at" --binary-hash "$binary_hash" \
    --ppsspp-executable-hash "$ppsspp_executable_hash" \
    --gate-asset-hash "$gate_asset_hash" \
    --input-manifest-hash "$(seal "$INPUT_MANIFEST")"
