# P3 — Windows guest 3D (venus WDDM) plan

Goal: GPU-accelerated 3D for the Windows 11 ARM64 guest on our from-scratch VMM,
reusing the host venus stack already proven with Linux (compute + image, 117-136
GB/s). The remaining piece is a Windows guest driver that speaks venus to our
virtio-gpu.

## The good news: our device is already feature-compatible
The community driver is `viogpu3d` on branch `viogpu3d-venus` of
`github.com/anonymix007/kvm-guest-drivers-windows-venus` (WIP, last venus commit
"[WIP] [viogpu3d] Add venus support"). Its `viogpu_adapter.cpp` bring-up
`AckFeature`s exactly what our device already offers:
- `VIRTIO_GPU_F_VIRGL` ✅ (we offer it)
- `VIRTIO_GPU_F_RESOURCE_BLOB` ✅ (P1b)
- `VIRTIO_GPU_F_CONTEXT_INIT` ✅ (P1a)
- `VIRTIO_GPU_CAPSET_VENUS` (id 4) ✅ (we expose it; smoke-verified)
- EDID ✅

So the device model built in P1a/P1b/P2 is the right shape — no known device
rework is required to *bring the driver up*.

## Known device-side gaps (small, host-side, in our control)
1. **PCI bind id.** `viogpu3d.inx` binds `PCI\VEN_1AF4&DEV_10F7`; our device (and
   the signed 2D `viogpudo`) present `DEV_1050`. A virtio-gpu has one id, so we
   either (a) env-gate our device to present `DEV_10F7` when running the 3D
   driver, or (b) patch the driver INF to bind `DEV_1050`. Prefer (a) so the
   proven 2D `viogpudo` path stays intact by default.
2. **VIRGL capset.** viogpu3d's D3D10 UMD path prefers the VIRGL/VIRGL2 capsets;
   we only expose VENUS (capset 4). The **Vulkan-via-venus** path (our proven
   one) should work; the **D3D10-via-virgl** path would need our host
   virglrenderer built with virgl (not just venus) + our device exposing the
   VIRGL capset. Target venus/Vulkan first (matches the Linux proof); virgl/D3D10
   is a later, separate lever.

## The real wall: the BUILD (physical constraint, honest)
`viogpu3d` is a WDDM kernel driver whose full build (per its `BUILDING.md`)
requires, INSIDE a Windows dev VM:
1. **Mesa built on Windows** (`meson -Dgallium-drivers=virgl -Dgallium-d3d10umd`)
   producing the user-mode DLLs — a major build in its own right, and
   Mesa-on-Windows-**ARM64** is not a beaten path.
2. **Visual Studio 2022 + WDK (ARM64 target)** to build `viogpu3d.sys` — GUI,
   interactive, multi-GB installers.
3. Test-signing set up, then inject + boot.

This cannot be driven by our headless boot-probe harness. It needs an
interactive Windows ARM64 developer environment. That is the genuine
physical limit for a fully-autonomous session.

## Realistic path across the wall
Two options, in preference order:

**Option A — build inside our own Windows 11 ARM64 guest (we already boot one).**
Our `--daily` Windows desktop works (networked, 4-core, 6 GB). Steps (multi-
session, mostly interactive setup that a human or a scripted unattended install
drives):
1. In the guest: install VS 2022 Build Tools + WDK (ARM64), Python, meson, ninja,
   git; build Mesa (virgl + d3d10umd, static CRT) → `MESA_PREFIX`.
2. `build_AllNoSdv.bat` in `viogpu/` → `viogpu3d.sys` (+ INF; patch bind id to
   `DEV_1050` or run our device as `DEV_10F7`).
3. `bcdedit /set testsigning on` (offline via our injector), inject the driver,
   boot with `BRIDGEVM_VIRTIO_GPU=1 BRIDGEVM_VIRTIO_GPU_3D=1` (+ the venus host
   env: in-process virglrenderer, `BRIDGEVM_VULKAN_LIB=MoltenVK`).
Could be partly automated with an unattended VS/WDK install image, but expect
interactive iteration.

**Option B — cross-build / external.** Provide a prebuilt `viogpu3d.sys` (ARM64,
test-signed) from any Windows ARM64 dev box; then this project injects + boots +
debugs — the parts our harness IS good at.

## Where WE have the edge (the reason this is worth doing)
The community driver is stalled largely because guest-side crashes are
undebuggable in a black box. We are not a black box:
- Our VMM traces every virtio-gpu command / fence / BAR access, and we added
  venus CS-error logging in virglrenderer — a guest driver bug becomes a
  host-replayable artifact.
- Serial KD over our proven UART (WinDbg serial) for kernel debugging.
- The host venus stack is FULLY VERIFIED (Linux), so any failure isolates to the
  guest driver — one unknown, not two.

## Bring-up ladder (once a driver binary exists)
1. `viogpu3d.sys` loads clean (Device Manager, no code 10/43). Keep `viogpudo`
   as the display path so the desktop never regresses while 3D stabilizes.
2. Guest `vulkaninfo` reports "Virtio-GPU Venus" (mirror the Linux P2 gate).
3. `vkcube`/a compute sample runs.
4. DXVK d3d11 → a real DX11 title (the Parallels-parity flag).
5. vkd3d-proton d3d12 (beyond Parallels).

## Status
- Driver source located + branch identified; device feature-compatibility
  confirmed by reading `viogpu_adapter.cpp`.
- Host venus stack proven (Linux, P2 + GPU-execution + 117-136 GB/s).
- BLOCKED on a Windows ARM64 build environment (VS+WDK+Mesa) — the physical
  constraint. Device-side prep (DEV_10F7 gating) is a small ready-when-needed
  task; not worth doing speculatively before a driver binary exists to test.

_2026-07-06. See [[bridgevm-hvf-engine-status]] and
docs/hvf-3d-engine-plan.md._
