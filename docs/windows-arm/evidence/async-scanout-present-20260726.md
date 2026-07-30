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
  `fb-rate.py` measures `display.fb` publication, not FPS. Note that the
  host-side `RESOURCE_FLUSH` rate is an *upper bound* on guest frame rate, not a
  measurement of it: it cannot distinguish "the guest rendered 30 frames" from
  "the guest rendered 60 and the host published 30".

## Open defect — swapchain-recreation loop (corrected 2026-07-26)

The synchronous deferred + IOSurface lane reproducibly fails to present a
PPSSPP window (2/2), while the same lane with async present enabled succeeds
(1/1). It is a pre-existing defect in that lane, not a regression introduced by
async present, and it blocks a normalised A/B.

**Correction.** An earlier revision of this document said both runs "composite
the desktop" and that only the title window was missing. That was wrong, and it
misdirected the diagnosis. The trace shows the guest *is* rendering at PPSSPP's
window size and failing to present, so it recreates its swapchain almost every
frame:

| run | `RESOURCE_CREATE_3D` | of which 1081×570 | `RESOURCE_FLUSH` | create/flush |
|---|---|---|---|---|
| `ab2-sync-1` (fail) | 2719 | **2174** | 2373 | **1.15** |
| `ab2-sync-2` (fail) | 2698 | **2179** | 2377 | **1.14** |
| `ab2-async-1` (pass) | 507 | 21 | 23959 | **0.02** |

1081×570 is PPSSPP's window size. In the failing runs each frame is a full
teardown/rebuild cycle, verified by dumping consecutive commands (seq
19113–19151 of `ab2-sync-1`), which repeats exactly:

```text
RESOURCE_CREATE_3D 1081x570 -> CTX_ATTACH_RESOURCE -> RESOURCE_ATTACH_BACKING
  -> SUBMIT_3D -> RESOURCE_DETACH_BACKING -> RESOURCE_UNMAP_BLOB(ERR)
  -> CTX_DETACH_RESOURCE -> RESOURCE_UNREF -> SET_SCANOUT -> RESOURCE_FLUSH
```

That cycle runs at ~4 Hz (2373 flushes / 600 s), so the window never stabilises
and `MainWindowHandle` stays 0 for the whole gate — hence
`reason=main-window-not-observed`. The guest was never idling on the desktop.

**Reusable health check.** The create-to-flush ratio separates the two states
without needing a screenshot:

- `create3d ≈ flush` (ratio ~1) → swapchain thrash, presentation is broken;
- `create3d ≪ flush` (ratio ~0.02) → healthy steady-state rendering.

For the automated end-of-run line, BridgeVM uses a deliberately loose
**healthy threshold of `create3d/flush <= 0.10`**. This leaves 5× headroom over
the observed healthy ~0.02 while remaining an order of magnitude below the
broken ~1 signature. `flush=0` reports `ratio=n/a healthy=false`: an idle run
cannot prove a healthy presentation loop.

Root cause of the present failure itself is still open; the swapchain rebuild is
the guest *reacting* to it (the shape Vulkan drivers produce on a persistent
`VK_ERROR_OUT_OF_DATE_KHR` / failed acquire), not the cause.

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
