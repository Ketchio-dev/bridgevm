# Real title on Venus — PPSSPP renders its full UI (2026-07-21)

## Status

**A real, unmodified, widely-used application renders on the BridgeVM 3D
stack.** PPSSPP 1.20.4 (the PSP emulator; official native Windows ARM64
build, unmodified binary) launches inside the Windows guest and draws its
complete UI — logo, tab bar, icon buttons, fonts, the PSP-symbol background
pattern, alpha-blended dialogs — composited on the Windows 11 desktop and
scanned out through Venus.

This is the first end-to-end proof with real third-party content rather than
a BridgeVM smoke: real textures, real font atlases, real UI widget rendering,
hundreds of draws per frame, sustained across seconds.

## What it exercises

PPSSPP auto-selected its **native Vulkan backend** (it ignored the staged
D3D11 config and picked Vulkan), so this run validates the pure guest-Vulkan
path — real app → Venus → virtio-gpu → BridgeVM HVF → render server →
MoltenVK → Apple GPU — with no DXVK translation layer. (The DXVK D3D11 path
is separately proven by the draw/present smokes; PPSSPP shows the Vulkan
path carries a real workload.)

Evidence frame: `~/BridgeVM/runs/venus-activate-120.41-ppsspp-*/ramfb/` (the
90 s virtio-gpu scanout sample shows the PPSSPP main menu). Render server log
confirms `vkr: ... context 35 (PPSSPP) with a valid instance`.

## First-run defect and fix: 64 MiB host-visible window

The first run hit `VK_ERROR_OUT_OF_HOST_MEMORY` at `vkBeginCommandBuffer`
after ~5.4 s. Root cause: the installed-boot runner configured the
virtio-gpu host-visible shared-memory window (`BRIDGEVM_VIRTIO_GPU_HOSTMEM_MIB`)
at **64 MiB**. That was ample for the single-16 KiB-blob smokes but a real
app maps far more host-visible memory (uniform ring, staging, dynamic
buffers) and exhausts a 64 MiB window within seconds, after which
`RESOURCE_MAP_BLOB` fails and the guest surfaces `OUT_OF_HOST_MEMORY`.

Fix: raise the runner default to **512 MiB** (BAR2 is 64-bit prefetchable, so
the larger window fits the PCI model; the value must stay a power of two and
< 4096). A concurrent leftover present-demo autostart (magenta window,
render-server context 31) was also sharing the window; bvinject now clears
stale demo Run keys so only the currently staged demo runs.

Confirmed: with the 512 MiB window PPSSPP renders its UI **stably for the
full 120 s bounded observation** with zero `OUT_OF_HOST_MEMORY`, a single
clean render-server context, and **21,522 SUBMIT_3D commands** over the run
(a real, sustained per-frame command stream). The leftover present-demo is
gone. Frame preserved as `ppsspp-real-title-120s.ppm` in the evidence dir.

## Remaining

- A gameplay workload (loading a homebrew/self-made PSP binary) would
  exercise the actual 3D game renderer beyond the UI; the UI already proves
  the pipeline end to end.
- Consider making the host-visible window size adaptive rather than a fixed
  default, and raise the smoke coverage to catch host-window exhaustion.

Evidence: `~/BridgeVM/runs/venus-activate-120.41-ppsspp2-*` (stable 512 MiB
run), `~/BridgeVM/runs/venus-activate-120.41-ppsspp-*` (first run + OOM).
