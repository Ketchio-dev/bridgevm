# A8 dynamic resize — fixed (2026-08-01)

Dynamic resize works. Three requests, all on the canonical image through
`scripts/verify-dynamic-resize.sh`:

| request | guest before | guest after | result |
| --- | --- | --- | --- |
| 1600x900 | 1280x1024 | 1600x900 | PASS |
| 1920x1080 | 1280x1024 | 1920x1080 | PASS |
| 1280x720 | 1280x1024 | 1280x720 | PASS |

The host agrees: in `a8-final-083610` the GPU trace ends on
`SET_SCANOUT 1600x900`, matching both the request and the guest's own geometry.

Three separate defects had to be cleared.

## 1. The EDID advertised almost no modes

`viogpu3d` builds its entire mode list from the EDID established-timing bits and
the eight standard-timing slots (`VioGpuVidPN::AddEdidModes`). BridgeVM set two
established bits and left every standard slot zero, so the guest enumerated
**one** mode.

Measured in-guest before: `Win32_VideoController` reported 800x600 and
`EnumDisplaySettings` returned `mode_count=1`. After filling in established
timings I/II, the manufacturer byte, and eight standard timings: **15 supported
source modes including 1600x900**, and the driver selects 1280x1024 by itself.

Modes wider than 2288 cannot be encoded in a standard-timing slot at all — the
field is `(h_active / 8) - 31` in one byte — so the list stops at 2048x1152 and
anything larger relies on the detailed timing descriptor.

## 2. Nothing in the guest applied the new mode

The miniport does its half: on `VIRTIO_GPU_EVENT_DISPLAY` it re-reads
`GET_DISPLAY_INFO` and calls `SetCustomDisplay`, which installs the geometry in
its mode table. It then signals `\BaseNamedObjects\VioGpuResolutionEvent<N>` and
stops.

The listener is upstream's `viogpuap.exe`, which is **not** in the 3D driver
package (`viogpu3d.sys`, `viogpu_d3d10.dll`, `vulkan_virtio.dll`,
`virtio_icd.arm64.json`, and the INF/CAT — no user-mode helper). The mode was
reachable and never chosen.

`scripts/win-assets/bvgpu-apply-host-resolution.ps1` is that listener, reduced to
the one job needed here: find the virtio-gpu display, read the published
geometry, and select it with `ChangeDisplaySettingsEx`.

`VIOGPU_GET_CUSTOM_RESOLUTION` was tried first and is a dead end on this build:
the shipped `viogpu3d.sys` returns `STATUS_NOT_SUPPORTED` (`0xC00000BB`) for it,
so the geometry is read from the monitor's supported source modes instead.

## 3. The verifier watched the wrong display

`verify-dynamic-resize.sh` read `PrimaryScreen.Bounds`, which reports
`\\.\DISPLAY1`. On this image that is the **Microsoft Basic Display Driver**,
pinned to a single 800x600 mode. The virtio-gpu adapter is a later `DISPLAY`
number with a real mode list:

```
BV-A8| \\.\DISPLAY1 cur=800x600    modes=1
BV-A8| \\.\DISPLAY2 cur=1280x1024  modes=28
```

Every earlier `guest=800x600` reading came from the wrong device. The verifier
now queries the virtio-gpu adapter directly.

## Ruled out

- **A host event/interrupt defect.** The host always accepted the request, armed
  the config-change interrupt, and the guest re-queried `GET_DISPLAY_INFO` — the
  notification path was complete from the start.
- **A shared root cause with the `vkCreateInstance` spin.** Resize still failed
  identically after the BAR2 fix.
