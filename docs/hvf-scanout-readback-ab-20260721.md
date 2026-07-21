# Scanout readback: phase split + pacing A/B (2026-07-21)

## Why

The verified perf audit of the PPSSPP run identified the synchronous
GPU→CPU scanout readback inside `RESOURCE_FLUSH` as the #1 measured host
cost: 4,449 events, 14.09 s of the 120 s run (11.7% of wall), avg 3.167 ms,
serializing the guest present. Two open questions gated the fix ladder:

1. Inside the 3.167 ms, how much is the GL transfer
   (`virgl_renderer_transfer_read_iov`) vs the CPU row-copy composite?
   The old timer wrapped both in one `duration_ns`.
2. How much does the existing pacing knob
   (`BRIDGEVM_VIRTIO_GPU_SCANOUT_READBACK_MS`, default 16 ms — which
   almost never engaged at the ~37 fps present rate: 3 throttled of 4,449)
   actually recover when raised?

## Changes

- `virtio_gpu.rs`: the `scanout_readback` trace event now carries
  `transfer_ns` (GL GPU→CPU pull) and `composite_ns` (CPU row-copy)
  alongside the unchanged total `duration_ns`.
- `bridgevm-cli` trace report: prints `Scanout readback transfer avg us` /
  `composite avg us`.
- installed-boot runner: a caller-supplied
  `BRIDGEVM_VIRTIO_GPU_SCANOUT_READBACK_MS` now rides `ENV_ARGS` into the
  probe (the launcher strips inherited `BRIDGEVM_*`, so exporting it alone
  never reached the device before).

## A/B results (PPSSPP autostart, 240 s watchdog, same disk/driver 120.41)

| pacing | readbacks | throttled | throttle% | readback total s | Δ vs 16 ms | transfer avg | composite avg |
|-------:|----------:|----------:|----------:|-----------------:|-----------:|-------------:|--------------:|
| 16 ms  | 4,129     | 1         | 0.02%     | 14.72            | —          | 3,039 us     | 459 us        |
| 33 ms  | 3,292     | 984       | 23.0%     | 12.10            | −17.8%     | 3,133 us     | 474 us        |
| 50 ms  | 2,927     | 1,198     | 29.0%     | 10.86            | −26.2%     | 3,154 us     | 484 us        |

Workloads are comparable run-to-run: flush attempts 4,130 / 4,276 / 4,125,
SUBMIT_3D 19,760 / 19,791 / 18,888; PPSSPP context alive until teardown in
all three. The 80 s scanout sample at 50 ms shows the full UI rendering
cleanly (static UI, so throttling costs motion smoothness only).

## Findings

- **The GL transfer dominates: ~85% of the readback path (3.0 of 3.6 ms);
  the CPU composite is ~13% (0.46 ms).** This settles the phase-split
  question: async-readback (P1) and Metal/IOSurface zero-copy (P2) target
  the right cost; optimizing the CPU composite is second-order.
- Pacing recovers real time but **sublinearly**: 50 ms recovered 26%, not
  the ~45–50% an even ~28 ms flush cadence would predict. Guest flushes are
  bursty; any gap larger than the window resets the throttle. Pacing is a
  useful stopgap lever, not a substitute for taking the readback off the
  flush path.
- Per-readback cost is stable across pacing (3.0–3.2 ms transfer), i.e. the
  transfer is not contention-priced; it is a fixed ~3 ms GL round trip at
  1024x768 (~0.85–0.88 GB/s effective).

## Next (per the audit fix ladder)

1. **P1**: decouple the readback from `RESOURCE_FLUSH` — respond OK
   immediately, run the readback at display cadence off the vCPU thread.
2. **P1**: host fence-poll thread + guest hybrid wait (short-spin +
   `NtDelayExecution`; `WaitOnAddress` refuted — host shmem writes don't
   wake it).
3. **P2**: zero-copy scanout — export the swapchain image via
   `VK_EXT_metal_objects` (already enabled in the virglrenderer patch) to an
   IOSurface bound to the host window, eliminating the transfer entirely.

Evidence: `~/BridgeVM/runs/venus-activate-120.41-readback{16,33,50}-20260721-*`.
