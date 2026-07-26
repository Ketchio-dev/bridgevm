# Asynchronous scanout present — live receipt, 2026-07-26

Depth-1 latest-wins asynchronous present (`BRIDGEVM_VIRTIO_GPU_ASYNC_PRESENT=1`)
measured against the matched synchronous lane on Windows 11 ARM64 / HVF /
virtio-gpu 3D / Venus → MoltenVK, driver `120.45.0.0`
(`~/BridgeVM/work/download-120.45-backing-only`, build `30169476359-66`).

## Headline

Async present **passes the real-title gate** and its epoch-cancellation logic is
confirmed working in production. A clean A/B speed comparison is **not** claimed:
the matched synchronous baseline failed its own title gate twice, so the two arms
did not run comparable workloads.

## Runs

| Run | Lane | Title gate | RESOURCE_FLUSH p50/p95/p99 µs | SUBMIT_3D p50/p95/p99 µs | flushes |
|---|---|---|---|---|---|
| `ab-sync-1-20260726` | plain (no deferred, no IOSurface) | **PASS** | 1602.083 / 3567.166 / 4493.458 | 52.708 / 2038.625 / 2981.208 | 22921 |
| `ab2-sync-1-20260726` | deferred + IOSurface | FAIL (no window) | 2.500 / 3.042 / 6.792 | 477.833 / 1506.459 / 2648.667 | 2373 |
| `ab2-sync-2-20260726` | deferred + IOSurface | FAIL (no window) | 2.583 / 3.125 / 7.417 | 486.750 / 1510.250 / 2778.750 | 2377 |
| `ab2-async-1-20260726` | deferred + IOSurface + **async present** | **PASS** | 6.000 / 11.583 / 14.583 | 74.500 / 1977.875 / 2578.209 | 23959 |

## What is proven

- **Correctness.** `ab2-async-1`: `status=0`, `guest_title_pass=1`,
  `driver_state_pass=1`, `BVGPU-REAL-TITLE-PASS`, PPSSPP `elapsed_ms=600248`
  with `main_window_observed=true` and `vulkan_virtio.dll` loaded,
  `SUBMIT_3D failure groups: none`, zero `Illegal`/`ret=22`.
- **The mailbox runs and is bounded.** 23946 `scanout_blit` against 23004
  `scanout_readback` in one run, with the guest continuously rendering.
- **Epoch cancellation fires for real.** 8 `scanout_present_cancelled` records,
  all `reason=stale_epoch`, on scanout rebinds — presents computed against a
  superseded scanout were discarded rather than composited. This is the
  ordering guarantee working under live load, not just in unit tests.
- **The flush is off the critical path.** Async `RESOURCE_FLUSH` p50 is 6.0 µs
  versus 1602.1 µs in the plain lane (`ab-sync-1`), a ~267x reduction.

## What is NOT proven

- **No clean async-vs-sync A/B.** The intended control (`ab2-sync-*`, same lane
  minus async) failed `reason=main-window-not-observed` on both attempts, doing
  ~2375 flushes against async's ~23959. Its 2.5 µs flush p50 measures an idle
  desktop, not a comparable workload, and comparing them would be meaningless.
  The 6.0 µs vs 1602.1 µs figure above is therefore against the *plain* lane and
  credits deferred + IOSurface + async together, not async alone.
- **No guest-visible FPS claim.** No guest frame-time instrumentation exists;
  `fb-rate.py` measures `display.fb` publication, not FPS.

## Open defect

The synchronous deferred + IOSurface lane reproducibly fails to present a
PPSSPP window (2/2), while the same lane with async present enabled succeeds
(1/1). Both reach `[stage4] done` and composite the desktop, so the driver and
renderer are healthy; only the title window is missing. This is a pre-existing
defect in that lane, not a regression introduced by async present, but it is
unexplained and blocks a normalised A/B.

## Reproduce

```
BRIDGEVM_VIRTIO_GPU_ASYNC_PRESENT=1 \
BRIDGEVM_VIRTIO_GPU_ASYNC_SCANOUT=1 \
BRIDGEVM_VIRTIO_GPU_IOSURFACE_SCANOUT=1 \
scripts/run-hvf-windows-installed-boot.sh \
  --target <clone>/disk.raw --vars <clone>/vars.fd \
  --evidence-dir <D> --watchdog-ms 1500000 --ram-mib 6144 --smp-cpus 4 \
  --virtio-gpu-3d --gpu-trace <D>/virtio-gpu.jsonl --gpu-trace-protocol venus \
  --viogpu3d-dir ~/BridgeVM/work/download-120.45-backing-only \
  --require-viogpu3d-readiness --require-real-title-gate
grep -E '^status|guest_title_pass' <D>/real-title-gate.txt
grep -E 'RESOURCE_FLUSH duration|failure groups' <D>/virtio-gpu-trace-report.txt
```

Async present must be visible in `run.log` as
`virtio-gpu: 3D scanout present dispatched asynchronously (depth-1, latest-wins)`;
absence of that line means the run measured the synchronous path.
