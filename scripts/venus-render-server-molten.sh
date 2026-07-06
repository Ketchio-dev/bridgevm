#!/bin/bash
# Force the venus render server to load MoltenVK directly (bypass the Vulkan
# loader's portability filtering) and log which vulkan lib dyld picks.
export DYLD_LIBRARY_PATH="$HOME/BridgeVM/3d/vk-direct"
export DYLD_PRINT_LIBRARIES=1
exec "$HOME/BridgeVM/3d/prefix/libexec/virgl_render_server" "$@"
