#!/usr/bin/env bash

# Bind each named boot workload to its exact runner arguments and identity.
perf_boot_profile() {
  local profile="$1" firmware_hash="$2" renderer_hash="$3"
  PERF_PROFILE_ARGS=(--virtio-net --enable-xhci --hda-coreaudio)
  case "$profile" in
    shipping-core-3d-boot-v1)
      PERF_PROFILE_ARGS+=(--virtio-gpu-3d --virtio-gpu-device-id 1050 --gpu-trace-protocol virgl --performance-risk aggressive)
      PERF_CONFIG_MATERIAL="shipping-core-3d-boot-v1;release;skip-build;daily;smp=4;ram=6144;virtio-net;xhci;hda-coreaudio;virgl;device=1050;aggressive"
      ;;
    shipping-core-3d-off-boot-v2)
      PERF_PROFILE_ARGS+=(--performance-risk balanced)
      PERF_CONFIG_MATERIAL="shipping-core-3d-off-boot-v2;release;skip-build;daily;smp=4;ram=6144;virtio-net;xhci;hda-coreaudio;virtio-gpu-3d=off;balanced"
      ;;
    *) return 1 ;;
  esac
  PERF_CONFIG_MATERIAL+=";display-fb=100ms;input-control;agent-ready;shutdown;watchdog=120000;firmware=$firmware_hash;renderer=$renderer_hash;warm-cache=clone-integrity-scan"
}
