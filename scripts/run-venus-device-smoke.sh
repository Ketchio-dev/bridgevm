#!/usr/bin/env bash
set -euo pipefail

BRIDGEVM_3D_DIR="${BRIDGEVM_3D_DIR:-"$HOME/BridgeVM/3d"}"
BRIDGEVM_VENUS_PREFIX="${BRIDGEVM_VENUS_PREFIX:-"$BRIDGEVM_3D_DIR/prefix"}"
export BRIDGEVM_VENUS_PREFIX

server_path="${RENDER_SERVER_EXEC_PATH:-}"
if [[ -z "$server_path" ]]; then
  while IFS= read -r candidate; do
    server_path="$candidate"
    break
  done < <(find "$BRIDGEVM_VENUS_PREFIX" -name virgl_render_server -type f -perm -111 -print 2>/dev/null)
fi
if [[ -z "$server_path" ]]; then
  candidate="$BRIDGEVM_3D_DIR/virglrenderer/build-venus/server/virgl_render_server"
  if [[ -x "$candidate" ]]; then
    server_path="$candidate"
  fi
fi
if [[ -z "$server_path" || ! -x "$server_path" ]]; then
  printf 'FAIL: could not find executable virgl_render_server\n' >&2
  exit 1
fi

export RENDER_SERVER_EXEC_PATH="$server_path"
export VK_ICD_FILENAMES="${VK_ICD_FILENAMES:-/opt/homebrew/share/vulkan/icd.d/MoltenVK_icd.json}"
export DYLD_FALLBACK_LIBRARY_PATH="/opt/homebrew/lib${DYLD_FALLBACK_LIBRARY_PATH:+:$DYLD_FALLBACK_LIBRARY_PATH}"

shim_path="tools/venus-host-probe/target/debug/libbridgevm_macos_shm_open_shim.dylib"
if [[ "$(uname -s)" == "Darwin" ]]; then
  mkdir -p "$(dirname "$shim_path")"
  cc -dynamiclib \
    -o "$shim_path" \
    tools/venus-host-probe/macos_shm_open_shim.c
  export DYLD_INSERT_LIBRARIES="$PWD/$shim_path${DYLD_INSERT_LIBRARIES:+:$DYLD_INSERT_LIBRARIES}"
fi

set +e
output="$(
  cargo run -p bridgevm-hvf --features venus --example venus_device_smoke 2>&1
)"
status=$?
set -e

printf '%s\n' "$output"
if ((status == 0)); then
  printf 'PASS: venus_device_smoke\n'
else
  printf 'FAIL: venus_device_smoke exited %d\n' "$status" >&2
  exit "$status"
fi
