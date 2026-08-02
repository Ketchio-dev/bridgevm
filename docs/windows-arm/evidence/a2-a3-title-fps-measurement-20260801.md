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

## What is not yet known

Why the title's presents are invisible to both PresentMon and DxgKrnl while its
3D submissions are plainly visible in the GPU trace. Two candidates, neither
tested yet:

1. The Venus swapchain reaches the screen by a path that does not raise the
   DxgKrnl flip/present events PresentMon keys on.
2. WDDM presentation statistics are simply not implemented by this miniport.
   Supporting observation: `\GPU Engine(*)\Utilization Percentage` and
   `\GPU Process Memory(*)\Local Usage` report **zero busy engines and zero
   instances for every process**, so this driver publishes no WDDM performance
   data at all.

Candidate 2 would also explain the DWM number being ~31 FPS rather than 60.

## Fixed along the way

The run script waited for a **count** of shared files before invoking the guest
launcher. The agent delivers files in arbitrary order, so the launcher could be
invoked before it arrived, failing with `exit=-196608` and silently producing no
title. It now waits for each file by name.

## Status

A2 and A3 stay open. The blocker is no longer "no content" — it is that no
available instrument attributes a frame rate to this title on this stack.
