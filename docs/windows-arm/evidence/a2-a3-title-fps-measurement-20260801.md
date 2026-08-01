# A2/A3 title FPS — measurement built, workload missing (2026-08-01)

With the `vkCreateInstance` spin fixed, PPSSPP 1.20.4 now runs on the Venus path
and the remaining gap for A2/A3 is a **process-attributed frame rate**. This
records what was built, what it measures, and why the gates are still open.

## The measurement

`scripts/win-assets/bvgpu-etw-title-fps.ps1`. Captures
`Microsoft-Windows-DxgKrnl` with `logman`, decodes with `tracerpt`, and keeps
only rows whose PID matches the title. Both tools ship with Windows.

Verified working in-guest: the session starts, ~320k rows decode, the PID filter
selects the title's rows (10k-17k of them), and a per-task rate breakdown prints.
The keyword mask must be `0xFFFF`; the initial `0x1` produced about one event per
second per task, which cannot be per-frame.

It reports `samples=0` with a reason rather than inventing a number.

## What it currently reports, and why

The title loads the intended graphics path on every run:

```
BV-FPS| target_pid=... window=PPSSPP v1.20.4
BV-FPS| graphics_path=vulkan_virtio.dll
```

But no task in the title's own rows falls in a plausible frame band. Across five
runs the shape is identical:

| task | rate | reading |
| ---: | ---: | --- |
| 68 (events 105/106, a start/stop pair) | 360-580/s | GPU packet submission, ~0.008 ms apart — not frames |
| 147 | 0.5-1.3/s | |
| 159, 1044, 11 | 0.3-0.6/s | |

A filter for anything between 20/s and 200/s returns **nothing**.

Two explanations were tested:

1. *Windowed apps present through DWM.* Confirmed as true but not the whole
   story: `dwm.exe` carries 125k rows with task 68 at 2302/s and task 105 at
   95/s. Running PPSSPP with `--fullscreen` did not move any task into the frame
   band for the title's own PID.
2. *The title is idle.* This is the actual cause. There is no game content on the
   image, so PPSSPP sits in its menu, and `--touchscreentest` did not change the
   picture either.

## Conclusion

The instrument is sound and the graphics path is proven loaded. What is missing
is a **rendering workload**: a real title with real content to run.

A2 and A3 stay incomplete. They cannot be closed by measuring harder; they need
content on the image, and the project-authored D3D11/Vulkan smokes do not
qualify as third-party titles.
