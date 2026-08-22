#!/usr/bin/env bash
# Product-mode VM process recreation for one B4 lane. Sourced by the case gate.
pointer_vm_launch() {
  RUN="$CASE/generation-$generation"; mkdir -p "$RUN"
  BRIDGEVM_TRACE_DCI5_EMISSION=1 BRIDGEVM_XHCI_REPORT_INTERVAL_MS=200 BRIDGEVM_VIRTIO_GPU_IOSURFACE_SCANOUT=1 BRIDGEVM_VIRTIO_GPU_ASYNC_PRESENT=0 \
  scripts/run-hvf-windows-installed-boot.sh --exit-on-reset \
    --target "$WORK/disk.raw" --vars "$WORK/vars.fd" --evidence-dir "$RUN" \
    --watchdog-ms 720000 --ram-mib 6144 --smp-cpus 4 --release --enable-xhci \
    --input-control "$INPUT" --display-export-fb "$RUN/active-scanout.fb" \
    --display-export-ms 100 --agent-service-control "$CTL" \
    --agent-share-host "$CASE/share" --agent-share-guest 'C:\BridgeVMPtr' --agent-share-ms 500 \
    --virtio-gpu-3d --gpu-trace "$RUN/virtio-gpu.jsonl" --gpu-trace-protocol venus \
    --viogpu3d-dir "$VIOGPU_DIR" > "$RUN/launcher.out" 2>&1 &
  pid=$!
}
pointer_vm_start_until_agent() {
  generation=0
  while (( generation <= 8 )); do
    pointer_vm_launch
    if wait_for '^BVAGENT SERVICE alive' 1 1200; then
      for path in run.log launcher.out virtio-gpu.jsonl active-scanout.fb active-scanout.fb.iosurface visible; do ln -sfn "generation-$generation/$path" "$CASE/$path"; done
      printf 'stable_generation=%s\n' "$generation" > "$CASE/reset.env"; return 0
    fi
    if kill -0 "$pid" 2>/dev/null; then fail 'agent service timeout'; fi
    set +e; wait "$pid"; rc=$?; set -e
    [[ "$rc" -eq 42 ]] || fail "helper exited $rc before agent"
    printf 'generation=%s exit=42\n' "$generation" >> "$CASE/reset-generations.log"
    generation=$((generation + 1)); : > "$CTL"; : > "$INPUT"
  done
  fail 'reset recreation limit exceeded'
}
pointer_vm_cleanup() {
  if [[ -n "${pid:-}" ]] && kill -0 "$pid" 2>/dev/null; then printf '%s\n' 'shutdown /s /t 3' >> "$CTL"; fi
  for _ in $(seq 1 60); do kill -0 "${pid:-0}" 2>/dev/null || break; sleep 1; done
  kill "${pid:-0}" 2>/dev/null || true; wait "${pid:-0}" 2>/dev/null || true
}
