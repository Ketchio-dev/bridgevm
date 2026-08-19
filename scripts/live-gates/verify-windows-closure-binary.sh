#!/usr/bin/env bash
set -euo pipefail
BINARY="$1"; RENDERER="$2"
codesign --verify --strict "$BINARY" >/dev/null 2>&1
codesign -d --entitlements :- "$BINARY" 2>&1 | grep -q 'com.apple.security.hypervisor'
BRIDGEVM_PROBE_PRINT_CAPABILITIES=1 "$BINARY" 2>&1 | grep -Fqx 'virtio_gpu_3d_compiled=true'
[[ "$(otool -L "$BINARY" | awk '/libvirglrenderer/{print $1; exit}')" == "$RENDERER" ]]
