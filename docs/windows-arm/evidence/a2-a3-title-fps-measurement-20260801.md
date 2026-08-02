# A2/A3 real-title FPS — where the measurement actually stands (2026-08-01)

A2 and A3 remain **incomplete**. This records what was proven, and corrects an
earlier conclusion in this document that turned out to be wrong.

## Correction to the earlier conclusion

An earlier revision concluded that the instrument was sound and the only missing
piece was game content. Content was then obtained and run. The frame band still
did not appear, so that conclusion was **wrong**: content was necessary but not
sufficient, and the measurement side had a real defect too.

## The title runs, with real third-party content, on the intended path

`cube.iso` — a PSP homebrew 3D demo from `hrydgard/pspautotests` — was shipped to
the guest and booted:

- window title `PPSSPP v1.20.4 - UCJS10041 : Cube sample`
- PPSSPP's own log (`--log=`, `--loglevel=3`): `Booted C:/BVPSP/cube.iso...`
- `graphics_path=vulkan_virtio.dll`, so the Venus path is loaded
- CPU is a steady ~10% of one core, sampled every 3 s over 24 s, with no drift
- GPU trace shows 7844 `SUBMIT_3D`, of which **6086 are on ctx 28**, distinct
  from DWM's ctx 7 (1542) — the title has its own busy 3D context

The application is genuinely rendering. That part is not in doubt.

## PresentMon works here, and reports zero frames for the title

`PresentMon 2.5.1` (x64, running under this ARM64 image's x64 emulation —
`xtajit64.dll` is present in System32) records successfully:

| application | rows in 20 s | p50 interval | p50 FPS |
| --- | --- | --- | --- |
| `dwm.exe` | 331 | 32.447 ms | **30.82** |
| `PPSSPPWindowsARM64.exe` | **0** | — | — |

Only one swapchain exists in the whole capture: `dwm.exe, 0x1FAD7B33EC0`. The
title owns none.

This is the key result. The instrument is not blind — it produces a clean,
plausible frame series for the compositor on the same capture — and it still
attributes **no presents at all** to the title.

## What was ruled out

- **Wrong ETW keyword.** `Microsoft-Windows-DxgKrnl` was captured with every
  keyword enabled (`0xFFFFFFFFFFFFFFFF`, up from `0xFFFF`). The event mix for the
  title's PID is unchanged: ids 105/106 at ~266/s each, then a gap to ~5/s.
  Nothing in a 20–200/s frame band.
- **Wrong provider.** `Microsoft-Windows-DXGI` records 3835 rows in 20 s and
  **0** for the title's PID, which is consistent with a Vulkan application that
  does not present through a DXGI swapchain.
- **The window is not composited.** It is visible, not minimized, in the
  foreground, and in session 1 alongside `dwm.exe`.
- **Missing modules.** The process has `vulkan-1.dll`, `vulkan_virtio.dll`,
  `dxgi.dll`, `dwmapi.dll` loaded (75 modules total).
- **The title is idle.** Disproven by the CPU series and by ctx 28's submit
  count; the earlier `--touchscreentest` and `--fullscreen` experiments are
  superseded by this.

## Resolved: it is the Vulkan presentation path, not the miniport

The two candidates were tested by switching PPSSPP to its **D3D11** backend
(`GraphicsBackend = 1`) and re-running the identical capture. PresentMon
immediately attributes frames to the title:

| backend | title swapchain | title samples | title p50 FPS |
| --- | --- | --- | --- |
| Vulkan | none | 0 | — |
| D3D11 | `0x265999C6C10` | 315 | 19.9 |

So the instrument and the miniport are both fine. **A Vulkan swapchain on this
stack does not raise the DxgKrnl present events** that PresentMon and every
DxgKrnl-based tool key on. That is candidate 1, and it means A2 cannot be
measured with a present-event-based instrument at all.

## A3: measurable, and currently below the gate

D3D11 is now measurable end to end, and it does not pass. Across three runs with
a 40 s warm-up and a 30 s window, the title's p50 sits at **19.5–20.0 FPS**,
against a gate of 30. Best-case intervals reach 40–54 FPS, so the ceiling is not
the limit.

PresentMon's own breakdown locates the cost, and it is not rendering:

| metric | p50 |
| --- | --- |
| `MsCPUBusy` | 0.17 ms |
| `MsGPUBusy` | 0.52 ms |
| `MsCPUWait` | 48.39 ms |
| `MsGPUWait` | 47.88 ms |
| `MsInPresentAPI` | **48.39 ms** |
| `MsRenderPresentLatency` | 229.44 ms |

Real work is ~0.7 ms per frame. The frame spends ~48 ms **inside the Present
call**. The host is not the one being slow: `SET_SCANOUT` is serviced in 0.004 ms
p50 and `RESOURCE_FLUSH` in 0.001 ms, with 937 scanouts spread evenly across the
run. Something in the present path is blocking, and 48 ms is suspiciously close
to three 16.7 ms refresh intervals.

`VSync = False` and `InflightFrames = 3` were tried and changed nothing
(19.45 -> 19.88 -> 19.99), so this is not the emulator's own frame cap.

Also note DWM measures 4.4-4.7 FPS p50 in these runs, far below the 30.8 it
showed on the Vulkan run. Whatever blocks the title's presents appears to hold
up the compositor as well.

## Fixed along the way

The run script waited for a **count** of shared files before invoking the guest
launcher. The agent delivers files in arbitrary order, so the launcher could be
invoked before it arrived, failing with `exit=-196608` and silently producing no
title. It now waits for each file by name.

## Status

Both stay open, for now-different reasons.

- **A2 (Vulkan)** cannot be measured by any present-event instrument on this
  stack, because the Venus swapchain raises no such events. A different
  instrument is needed, not a different workload.
- **A3 (D3D11)** is measurable and reads **19.5–20.0 FPS p50** against a gate of
  30. The deficit is a ~48 ms stall inside Present, not rendering cost.
