#!/usr/bin/env bash
set -euo pipefail

BRIDGEVM_3D_DIR="${BRIDGEVM_3D_DIR:-"$HOME/BridgeVM/3d"}"
BRIDGEVM_VENUS_PREFIX="${BRIDGEVM_VENUS_PREFIX:-"$BRIDGEVM_3D_DIR/prefix"}"
export BRIDGEVM_VENUS_PREFIX

export VK_ICD_FILENAMES="${VK_ICD_FILENAMES:-/opt/homebrew/share/vulkan/icd.d/MoltenVK_icd.json}"
export DYLD_FALLBACK_LIBRARY_PATH="/opt/homebrew/lib${DYLD_FALLBACK_LIBRARY_PATH:+:$DYLD_FALLBACK_LIBRARY_PATH}"

cargo build --manifest-path tools/venus-host-probe/Cargo.toml

shim_path="tools/venus-host-probe/target/debug/libbridgevm_macos_shm_open_shim.dylib"
if [[ "$(uname -s)" == "Darwin" ]]; then
  mkdir -p "$(dirname "$shim_path")"
  cc -dynamiclib \
    -o "$shim_path" \
    tools/venus-host-probe/macos_shm_open_shim.c
  export DYLD_INSERT_LIBRARIES="$PWD/$shim_path${DYLD_INSERT_LIBRARIES:+:$DYLD_INSERT_LIBRARIES}"
fi

tools/venus-host-probe/target/debug/venus_capset_probe \
  --protocol virgl \
  --allow-unavailable
