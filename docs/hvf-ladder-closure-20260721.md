# Perf-ladder closure — all four remaining rungs driven to ground (2026-07-21)

Session goal: eliminate every remaining rung of the audit fix ladder. Final
states, with evidence:

## Rung 1 — Metal/IOSurface zero-copy scanout: LANDED, pixel-exact

- virglrenderer grows `virgl_renderer_bridgevm_scanout_blit_iosurface`
  (+ checksum/dump diagnostics): a GPU blit from the vrend scanout texture
  into a cached global IOSurface via `CGLTexImageIOSurface2D`, in-process
  (vrend's CGL context lives in the probe — no cross-process surface
  passing needed). `BRIDGEVM_VIRTIO_GPU_IOSURFACE_SCANOUT=1` blits once per
  armed frame; the CPU readback demotes to a pace-able evidence feed; the
  surface's global ID is published beside `BRIDGEVM_DISPLAY_EXPORT_FB` for
  a windowed viewer to bind `layer.contents` (viewer wiring = the remaining
  consumer step, needs a GUI session to validate).
- **Verified pixel-exact: 1,946/1,946 same-frame FNV checksums match**
  (run `venus-activate-120.41-iosurf5`); blit avg **55.9 us** vs the 3,167 us
  readback it replaces (57x). A day of false mismatches traced to a typo'd
  FNV offset basis in the C helper — the blit was byte-identical from the
  first buffer dump.
- Side discovery: the blit's flush also cut the remaining CPU readback from
  ~3.0 ms to ~0.9-1.1 ms (pipeline already drained at glGetTexImage time).

## Rung 2 — GPU device thread: SUPERSEDED BY DATA, design banked

- The rung existed to recover vCPU-thread time from the readback; the
  deferred+blit work removed that cost. `command` trace events now carry
  `duration_ns`, so the one remaining candidate (SUBMIT_3D GL decode) is
  measurable; `docs/hvf-gpu-thread-design-20260721.md` holds the SPSC
  design and the reopen gate (>5%/s of a core in submit decode).

## Rung 3 — x64 DXVK under Windows x64 emulation: END-TO-END PROVEN

- Built in CI (new `x64-userstack` job): x64 venus ICD (`vulkan_virtio.x64.dll`,
  native clang-cl, same mesa ref + full patch chain) + x64 Khronos loader
  (`vulkan-1.dll`). Staged with the x64 DXVK dlls + newly cross-compiled
  x64 smokes into `C:\BridgeVM\dxvk-x64`; bvinject registers the ICD
  manifest under `SOFTWARE\Khronos\Vulkan\Drivers` (elevated processes
  ignore `VK_DRIVER_FILES` — loader secure-env policy; the registry is how
  the ARM64 ICD is found too).
- **`bridgevm-d3d11-present-smoke-x64.exe` exits 0** (run
  `venus-activate-120.42-grand7`): x64 process → x64 DXVK 3.0.2 → x64
  loader → x64 venus ICD → D3DKMT across the emulation boundary → viogpu3d
  KMD → Venus → MoltenVK. The loader log shows the ARM64 ICD skipped as
  "wrong bit-type" and the x64 ICD loaded — exactly the dual-arch shape the
  engine plan (§3c) called for. The x64 draw-smoke variant still fails
  (vb=255/novb=1, present passes) — narrower follow-up, not a path blocker.

## Rung 4 — guest hybrid fence wait: MECHANISM LANDED, THEN REVERTED ON DATA

- The external-builder blocker fell: the CI chain (Ketchio-dev builder) is
  drivable end to end from this machine. 120.42 shipped the extended
  `vn_relax` patch (spin-order env knob `VN_RELAX_WIN32_SPIN_ORDER`,
  1 ms `NtSetTimerResolution` floor on ladder entry, defaults preserving
  120.41 exactly) and installed cleanly (bound `120.42.0.0`, bench gate
  PASS, wait_avg 158-206 us).
- **But PPSSPP's Vulkan backend crashes at startup on every post-upgrade
  cycle**, and an exhaustive bisect exonerated every binary-level suspect:
  - WER pins the fault: `vulkan_virtio.dll`, c0000005 at offset 0x2c9b04
    (-64 build), during instance creation.
  - The vn_common patch is NOT the cause: 120.43 reverts it byte-for-byte
    at the source level and still crashes.
  - Stale-ICD loading was real but not the cause: the Khronos registry kept
    serving the 120.42 DriverStore dir after upgrades (bvinject now
    overwrites every stored `viogpu3d.inf_arm64_*` ICD copy), yet the crash
    survives with the **known-good -60 ICD binary** in every load path.
  - The KMD source is identical (both -60 and -64 checked out d780b2b) and
    the CI toolchain is identical (LLVM 20.1.8, MSVC 14.44.35207) — the
    toolchain-drift theory died on log evidence.
  - A no-host-flags control run still crashes: the host-side IOSurface/
    async/instrumentation work is exonerated.
  - Remaining consistent explanation: **guest driver-state damage from the
    upgrade cycle itself** (38 accumulated DriverStore packages, per-build
    test certs, stage2 display-config resets). Recovery path for the next
    session: WinPE `pnputil`-based DriverStore cleanup + reinstall of the
    archived known-good 120.41 package, or restoring the guest image to a
    pre-upgrade state. The deeper lesson stands either way: **all smoke
    gates pass while a real title crashes — the driver-promotion ladder
    needs a real-title gate (PPSSPP launch + N seconds of flushes) before
    any package is considered good.**

## Meta-lessons recorded

- Inject vs activate UEFI vars differ by boot order; activate vars silently
  boot the installed OS and bvinject never runs (exit still 0).
- The firstboot chain's ONSTART continuation task did not re-fire across
  boots after a same-boot boot-gate refusal; re-arming RunOnce via a fresh
  inject is the reliable resume path.
- The builder's `driver_ref` is a moving branch — pin it for reproducible
  driver bisection.

Evidence: `~/BridgeVM/runs/venus-activate-120.41-iosurf{3,4,5}-*`,
`venus-activate-120.42-grand{3,4,6,7}-*`, `venus-inject-120.42-x64{b,d}-*`,
builder runs 29855960079 (-61), 29857550580 (-63), 29864055824 (120.43).
