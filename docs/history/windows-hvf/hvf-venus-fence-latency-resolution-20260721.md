# Venus guest fence latency resolution (2026-07-21)

## Status

Resolved. The guest Windows ARM64 draw/fence loop went from **45-47 fps to
4,343 fps** — 77% of the host-native MoltenVK rate — with a single guest
driver change (package `120.41.0.0`).

```text
before (120.40):  fps=45.49-47.11  min=1.2-2.0 ms  max=81-88 ms  wait_avg ~20 ms
after  (120.41):  fps=4343.73      min=190 us      max=367 us    wait_avg 181 us
host  (MoltenVK): fps=5646         min=138 us      max=360 us
```

## Root cause

The presumed "~120x Venus transport overhead" was almost entirely **Windows
timer quantization in the guest fence wait**, not transport cost:

- Venus fences complete via a host-written feedback slot that the guest
  driver polls with `vn_relax` backoff (`vn_common.c`).
- The fence relax profile busy-yields only 15 iterations before entering the
  sleep ladder (`base_sleep_us = 160` doubling upward).
- Mesa's `os_time_sleep` on Windows rounds up to `Sleep(ms)`, and `Sleep`
  quantizes to the system timer resolution — **up to 15.6 ms by default**.
- The real fence roundtrip on this stack is ~180 us, so any wait that
  reached the sleep ladder inflated to one-or-more 15.6 ms quanta: measured
  min ~1.5 ms (lucky yield-phase completions), average ~20 ms (1-2 quanta),
  max ~88 ms (escalated ladder steps).

## Fix

`mesa-venus-win32-fence-relax.patch` (builder chain, driver 120.41): on
win32, the fence/semaphore/query relax profile busy-yields through order 11
(2047 yields) before the first sleep, so typical waits complete in the yield
phase and never touch the quantized `Sleep`.

DXVK-based apps benefit through the same `vkWaitForFences` path; DXVK's own
sleep utility additionally raises the timer via `NtSetTimerResolution` where
it sleeps itself.

The bench (`bridgevm-vulkan-draw-smoke.c`) now reports a submit/wait phase
split (guest: submit ~0 us enqueue, wait 181 us) and has a
`BV_BENCH_TIMER_RES=1` knob that raises the Windows timer to 1 ms for A/B
runs; with the yield-phase fix the knob is no longer needed for the common
case.

## What this means for the remaining perf ladder

The Venus fence roundtrip itself is ~1.3x host native, so the old
fence-batching/submit-coalescing leads are now second-order for this
workload. The next real cost centers are the CPU-copy GDI present path
(per-frame backbuffer readback) and real-content transport volume, to be
baselined against an actual DX11 title.

Evidence: `~/BridgeVM/runs/venus-activate-120.41-fence-relax-*` (all Vulkan
gates PASS, checksums unchanged).
