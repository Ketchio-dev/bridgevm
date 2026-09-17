#!/usr/bin/env bash
set -euo pipefail
REPO="$(cd "$(dirname "$0")/../.." && pwd)"
source "$REPO/scripts/live-gates/hvf-boot-performance-profile.sh"
perf_boot_profile shipping-core-3d-off-boot-v2 a b
args=" ${PERF_PROFILE_ARGS[*]} "
[[ "$args" != *" --virtio-gpu-3d "* && "$args" == *" --performance-risk balanced "* ]]
[[ "$PERF_CONFIG_MATERIAL" == *"virtio-gpu-3d=off"* ]]
perf_boot_profile shipping-core-3d-boot-v1 a b
args=" ${PERF_PROFILE_ARGS[*]} "
[[ "$args" == *" --virtio-gpu-3d "* && "$args" == *" --performance-risk aggressive "* ]]
if perf_boot_profile unknown a b; then exit 1; fi
echo "HVF boot performance profiles: PASS"
