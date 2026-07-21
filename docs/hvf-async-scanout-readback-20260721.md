# Deferred scanout readback — flush path decoupled (2026-07-21)

## What changed

`BRIDGEVM_VIRTIO_GPU_ASYNC_SCANOUT=1` (opt-in, runner forwards it) moves the
3D scanout GL readback off the guest's `RESOURCE_FLUSH`:

- The flush handler arms a pending readback (rect-unioned per resource) and
  responds `OK_NODATA` immediately — the guest present no longer waits the
  ~3.4 ms readback (85% of which is the GL GPU→CPU transfer, per the phase
  split).
- The per-exit drain (`poll_virtio_gpu_fences`) services the pending
  readback. A `fresh` guard skips the drain pass of the exit that armed the
  flush, so the response reaches the guest and the vCPU resumes at least
  once before this thread pays for the readback.
- Pacing changes semantics under deferral: a not-yet-due pending frame is
  **held, not dropped** (sync throttling discards the update; deferred mode
  delays it), so the newest frame always lands.
- `scanout_readback` trace events carry `deferred:0|1`; the CLI report
  prints `Scanout readbacks deferred-serviced`.

## Validation (PPSSPP autostart, pacing 16 ms, driver 120.41)

Run `venus-activate-120.41-asyncscan16-20260721-133656`:

- 4,150 readbacks, **all deferred-serviced** (`deferred:1` on every event);
  0 throttled (held-not-dropped semantics).
- Full PPSSPP UI renders cleanly (80 s scanout sample), all gates PASS.
- Flush cadence ~34.6/s vs ~34.4/s sync baseline — DWM's present cadence is
  self-paced, so the win is **per-present latency** (response no longer
  carries the 3.4 ms readback), not present throughput.
- Coalescing engaged only 4 times (exits far outpace flushes) — expected.
- Unit coverage: defer/service/fresh-guard, coalescing, held-not-dropped
  (`virtio_gpu.rs` tests), 719-test crate suite green.

Honest scope note: the readback still runs on the vCPU thread (total
~14.3 s per 120 s unchanged); this change removes it from the guest present
critical path only. Recovering the CPU time needs the readback off-thread —
blocked on virglrenderer proxy thread-safety (all FFI calls currently ride
the vCPU thread by design; the safe architecture is a dedicated GPU device
thread, an L-size refactor) — or eliminated entirely by the Metal/IOSurface
zero-copy scanout (P2 in the audit ladder).

## Also in this change

- The runner archives `C:\BridgeVM\*.log` from the target disk into
  `<evidence>/guest-logs/` after each 3D run (read-only raw attach,
  best-effort). This closes the audit's evidence gap: the guest microbench
  log (`bvgpu-vulkan-draw.log`, e.g. `wait_avg_us=189`) now lives in the
  host evidence tree instead of only inside the image.

## Deferred (with rationale)

- **Host fence-poll thread** (audit P1b): `virgl_renderer_context_poll`
  from a second thread races the single render-server proxy connection;
  virglrenderer's proxy is not documented thread-safe and the C source is
  not vendored to verify. Fence latency is already ~1.3x host (181-189 us
  vs 138 us) — second-order. Revisit as part of a GPU-device-thread
  refactor.
- **Guest hybrid fence wait** lives in the external Mesa builder chain
  (`vn_relax` patch), not this repo.

Evidence: `~/BridgeVM/runs/venus-activate-120.41-asyncscan16-20260721-133656/`.
