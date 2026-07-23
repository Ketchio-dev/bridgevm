#!/usr/bin/env bash
set -euo pipefail

BRIDGEVM_3D_DIR="${BRIDGEVM_3D_DIR:-"$HOME/BridgeVM/3d"}"
BRIDGEVM_VENUS_PREFIX="${BRIDGEVM_VENUS_PREFIX:-"$BRIDGEVM_3D_DIR/prefix"}"

lib_path="$BRIDGEVM_VENUS_PREFIX/lib/libvirglrenderer.dylib"
[[ -f "$lib_path" ]] || { echo "FAIL: missing $lib_path" >&2; exit 1; }

otool -L "$lib_path"
renderer_init_count="$(nm -U "$lib_path" | grep -c 'virgl_renderer_init' || true)"
((renderer_init_count > 0)) || {
  echo "FAIL: $lib_path does not export virgl_renderer_init" >&2
  exit 1
}

server_path=""
while IFS= read -r candidate; do
  server_path="$candidate"
  break
done < <(find "$BRIDGEVM_VENUS_PREFIX" -name virgl_render_server -type f -perm -111 -print)
if [[ -z "$server_path" ]]; then
  candidate="$BRIDGEVM_3D_DIR/virglrenderer/build-venus/server/virgl_render_server"
  [[ -x "$candidate" ]] && server_path="$candidate"
fi
[[ -n "$server_path" && -x "$server_path" ]] || {
  echo "FAIL: missing executable virgl_render_server" >&2
  exit 1
}

vkr_count="$(nm -U "$server_path" | grep -c 'vkr_' || true)"
((vkr_count > 100)) || {
  echo "FAIL: expected >100 vkr_ symbols in $server_path, found $vkr_count" >&2
  exit 1
}

printf 'PASS: native Venus artifacts\n'
printf '  library: %s\n' "$lib_path"
printf '  render server: %s\n' "$server_path"
printf '  vkr_ symbols: %s\n' "$vkr_count"
