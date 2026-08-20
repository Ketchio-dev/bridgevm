# Windows 1.0 closure — F1–F4 live gate PASS (2026-08-20)

The sealed-input `t7-windows-closure` tier passed on exact head
`439dcb0a1b99bea968b2979433d396ed22b2e997`.

- Job: `20260820-072920-83022-13640`, `result=pass`, `exit_code=0`
- Receipt SHA-256 (private == public):
  `fefad903a56fa2f1c63e6067079208928d1dae1f415ff966611629ea37727a97`
- `passes=4`, `failures=0`, `injector_boot_observed=true`,
  `module_identity_verified=true`
- Sealed inputs: image `dc42fa45…`, target vars `c61e2136…`, injector vars
  `2307d7ee…`, injector `5eaedee1…` (assets digest `d8354715…`), agent
  `b7820834…`, virglrenderer `acb4b5bb…`, MoltenVK `7843d3de…`, probe binary
  `66f54568…`, viogpu3d driver tree `94bfe4c2…`
- The tier injected into an APFS clone, retained the prepared pair immutably
  under `~/BridgeVM/prepared/windows-1.0/` by post-injection hashes, and proved
  F1–F4 on a second clone; source media hashes were re-verified unchanged
  before and after both boots.

## What each criterion showed live

**F1 — driver-loadable image.** The guest reported
`BVF1 testsigning=True viogpu_status=OK viogpu_problem=0 vioserial_status=OK
vioserial_problem=0 agent_sha256=b7820834…` and a real mode list:
`BVF1MODE device=\\.\DISPLAY2 current=1280x1024 modes=28 has_1600x900=True
list=640x480,…,1600x900,…,2048x1152`.

**F2 — resolution adoption.** After the host `RESIZE 1600x900` and the guest
mode apply, the guest reported `BVF2 device=\\.\DISPLAY2 current=1600x900
modes=28 has_1600x900=True`, and the virtio-gpu trace recorded 172
`SET_SCANOUT` commands answered `OK_NODATA` with `rect_w=1600, rect_h=900`.

**F3 — shipped Coherence verbs.** One continuous round-trip over the agent
channel: `WINLIST` listed `WIN 66298 5140 … (Untitled - Notepad)`;
`WINBOUNDS 66298 50 60 700 500 -> OK WINBOUNDS`;
`WINFOCUS 66298 -> OK WINFOCUS`; the guest-side probe confirmed
`BVWINDOW hwnd=66298 exists=True pid=5140 rect=50,60,700,500
foreground=66298`; `WINCLOSE 66298 -> OK WINCLOSE`; and the second `WINLIST`
no longer listed the window.

**F4 — glyph observation, measured bound.** An active virtio-gpu checkpoint
(`virtio-gpu-checkpoint-f4-notepad-focused-0000.xrgb8888`, never RAMFB) was
captured with the Notepad window focused and typed into, retained as
`captures/f4-notepad-focused.ppm`, SHA-256
`9b7fce8dc622185402c803169d82e4b8a2793f000d1365bca8e3b856a2217f3b`.
Inspection shows a 1600x900 all-black frame: with the Venus/3D presentation
path owning the display, the 2D scanout byte-buffer that this checkpoint dumps
is not the presented image. The receipt therefore records
`f4_glyph_observation=measured-no-recognized-glyphs` — the channel, window
state and capture pipeline are proven; pixel-level glyph verification needs
the 3D scanout readback path and remains an explicit 1.x follow-up, not a
silent pass.

## Defects measured and fixed to reach this

1. **Guest CRLF broke every `$`-anchored proof assertion.** Guest stdout is
   relayed verbatim (CRLF intact) while agent protocol lines are LF-only, so
   `grep '…$'` could never match guest output — including the firstboot
   readiness gate, which would have consumed its whole 45-minute wait. Fixed
   with `\r?$` anchors; pinned by mutation-tested policy-smoke assertions
   (`7a851c9`).
2. **WINLIST read its title from a field the host never prints.** The host
   emits 10 fields; the script read `$11`, so no window was ever found and F3
   was unreachable. Fixed to `$10` (`7a851c9`).
3. **The PS 5.1/ARM64 EnumWindows callback hung the agent.** WINLIST never
   replied and the channel wedged behind it. Replaced with the live-proven
   `Get-Process` main-window walk (`0c60590`).
4. **`$w` clobbered the `$W` window-API type.** PowerShell variables are
   case-insensitive; the first loop iteration replaced the type with an
   IntPtr, the swallowed throw meant `WINEND` never went out, and the host
   stayed in-flight forever. Harvested from the guest's own `bvagent.log`,
   reproduced byte-for-byte in a pwsh simulation, fixed as `$hw` with a
   `try/finally` guaranteeing the terminator (`574c16e`).
5. **The F2 pattern demanded text that the guest never emits** (`.* modes=`
   after `current=1600x900`), and **the Window proof assigned the read-only
   automatic `$pid`**, which throws under Stop preference. Both fixed
   (`439dcb0`).

Earlier root causes proving the path (previous sessions): the two racing
firstboot entry points (`e610e5b`), booting the injector with its own vars
(`11a795f`), and the host virtio-gpu geometry bound that refused every real
`SET_SCANOUT` until the host resize request widened it (measured in job
`20260820-002138`, lifted by the tier's `RESIZE 1600x900`).
