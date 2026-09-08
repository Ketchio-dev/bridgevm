#!/usr/bin/env bash
# Link the real patched renderer translator; deliberately no source-text assertions.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SRC="${BRIDGEVM_VIRGL_SOURCE:-${BRIDGEVM_3D_DIR:-$HOME/BridgeVM/3d}/virglrenderer}"
BUILD="${BRIDGEVM_VIRGL_BUILD:-$SRC/build-venus}"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
includes=("$BUILD" "$BUILD/src" "$BUILD/src/gallium" "$SRC/src" "$SRC/src/mesa"
  "$SRC/src/mesa/pipe" "$SRC/src/mesa/compat" "$SRC/src/gallium/include" "$SRC/src/gallium/auxiliary")
args=(); for path in "${includes[@]}"; do args+=("-I$path"); done
read -r -a epoxy_flags <<< "$(pkg-config --cflags --libs epoxy)"
cc "$ROOT/tests/integration/virgl-buffer-sampler-translate.c" "${args[@]}" \
  "$BUILD/src/libvirgl.a" "$BUILD/src/gallium/libgallium.a" "$BUILD/src/mesa/libmesa.a" \
  "${epoxy_flags[@]}" -framework OpenGL -framework IOSurface -framework CoreFoundation \
  -framework Metal -framework Foundation -o "$TMP/translator"
"$TMP/translator"
