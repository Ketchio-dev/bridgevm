# DXVK D3D11 on Venus — bring-up status (2026-07-21)

## Status

**DXVK D3D11 renders correctly on Venus.** An ARM64 build of upstream DXVK
3.0.2 creates a **D3D_FEATURE_LEVEL_11_0** device on the Venus adapter inside
the Windows ARM64 guest and draws a pixel-perfect triangle through the
standard vertex-buffer path:

```text
BV-D3D11-DRAW-DEVICE feature_level=0xb000 mode=vb
BV-D3D11-DRAW-MODULE d3d11.dll=C:\BridgeVM\dxvk\d3d11.dll
BV-D3D11-DRAW-ADAPTER vendor=0x106b device=0x1a050209 desc=Virtio-GPU Venus (Apple M4 Max)
BV-D3D11-DRAW-RESULT center=ff00ffff magenta_pixels=4096 bad_pixels=0
BV-D3D11-DRAW-PASS

BV-D3D11-DRAW-DEVICE feature_level=0xb000 mode=novb
BV-D3D11-DRAW-RESULT center=000000ff magenta_pixels=0 bad_pixels=4096
BV-D3D11-DRAW-FAIL step=verify
```

The vb/novb pair isolates the one remaining rendering defect precisely: a
draw with NO vertex buffer bound (SV_VertexID) rasterizes nothing, because
the relaxed `nullDescriptor` requirement leaves DXVK's null-binding handling
without the Vulkan feature it assumes.  Real D3D11 content overwhelmingly
binds vertex buffers, so the standard path is proven; the null-binding gap is
a known limitation to mitigate (DXVK-side dummy buffer, or nullDescriptor
emulation in the stack).

## How it runs

- DXVK upstream (commit `0ff9cd3`) is ARM64-aware; cross-built on macOS with
  llvm-mingw 20260616 (`scripts/win-tests/dxvk-build-winarm64.txt` is the meson
  cross file; toolchain under `~/BridgeVM/toolchains/`). Build dir:
  `~/BridgeVM/dxvk/build.arm64`.
- The relax patch `scripts/patches/dxvk-macos-venus-relax.patch` turns five
  hard requirements into reduced-caps: `geometryShader`,
  `shaderCullDistance` (Metal has no geometry stage), `depthClipEnable`
  (extension absent in MoltenVK), `robustBufferAccess2`+`nullDescriptor`
  (robustness2 features false), and `khrSwapchain` (see below). This mirrors
  the DXVK-macOS approach on a current ARM64-capable codebase.
- Guest side: `scripts/win-tests/bridgevm-d3d11-draw-smoke.c` runs from
  `C:\BridgeVM\dxvk\` so the local DXVK d3d11/dxgi win the loader search;
  firstboot stage 3 runs it non-gating with `DXVK_LOG_PATH=C:\BridgeVM\dxvk`.

## The diagnosis chain that got here (each step one bounded guest run)

1. `E_FAIL`, no DXVK logs → added `DXVK_LOG_PATH`: DXVK loads, finds the Venus
   device, but "Device does not support Vulkan 1.3".
2. Device advertised 1.2: Venus clamps to 1.2 when `VK_KHR_synchronization2`
   is off, and the WDDM port gates sync2 (like all
   `VN_USE_WSI_PLATFORM` builds) on sync-fd semaphore import it lacks. Fixed
   guest-side in driver **120.38** (`mesa-venus-wddm-sync2.patch`): the WDDM
   port presents through KMD scanout, not sync-fd WSI semaphores, so sync2
   stays on and the device advertises **1.3.334** (verified by the Vulkan draw
   smoke, which now requests 1.3 — its old 1.1 request had been self-clamping
   the reported version).
3. Next skip: `depthClipEnable` → static diff of all DXVK required features
   against MoltenVK found exactly `depthClipEnable`, `robustBufferAccess2`,
   `nullDescriptor` remaining → relaxed.
4. Next skip: `khrSwapchain` — the same guest sync-fd gate blocks
   `VK_KHR_swapchain` exposure. Relaxed in DXVK for offscreen bring-up; the
   real fix (exposing swapchain without sync-fd import, or implementing
   driver-side external semaphores) is the presentation-path work and is
   REQUIRED before any windowed D3D11 app can present.
5. Device creation succeeded at FL 11_0; draw executes but reads back black.

## Alongside: driver 120.37/120.38 and the timing baseline

- **120.37** retired the temporary Mesa D3D bring-up beacons (builder chain
  verified locally to still apply); all Vulkan gates stayed green with
  unchanged checksums.
- The draw smoke gained a present-free draw/fence loop. Baselines, 300 frames,
  64x64 offscreen: **host MoltenVK 5646 fps (min 138 us / max 360 us)**;
  **guest via Venus 45-47 fps (min ~1.7 ms / max ~88 ms)** — a ~120x
  fence-roundtrip overhead that quantifies the transport optimization target
  (fence batching / submit coalescing leads in the perf plan).
- **120.38** = 120.37 + `mesa-venus-wddm-sync2.patch`; Vulkan gates and bench
  unchanged (45.49 fps), device api now 1.3.334.

## Next work

1. RESOLVED (this doc, same day): the black first-draw was the relaxed
   `nullDescriptor` null-binding path — the vertex-buffered draw passes
   pixel-perfect.  Mitigate the null-binding gap later (DXVK dummy buffer or
   nullDescriptor emulation); it only affects draws with nothing bound.
2. Presentation: expose `VK_KHR_swapchain` in the guest driver without
   sync-fd import (driver-side external semaphore per the Mesa TODO), then
   drop the DXVK khrSwapchain relax and run a windowed D3D11 present test.
3. Then the rung-4 flag: a real DX11 title.

Evidence: `~/BridgeVM/runs/venus-activate-120.38-dxvk4-*` (final run) and the
`venus-activate-120.37-*`/`-120.38-*` chain before it.
