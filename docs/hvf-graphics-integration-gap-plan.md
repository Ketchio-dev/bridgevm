# BridgeVM HVF vs Parallels — Graphics/Integration Gap & Roadmap

> Historical strategy snapshot. Its guest-feature and Windows ARM64 driver
> availability claims are superseded by `STATUS.md` and
> `docs/hvf-p3-windows-3d-plan.md`; as of 2026-07-10 test-signed injection-ready
> ARM64 viogpu3d packages exist, while live bind/trace/render proof remains open.

Honest, engineering-grounded analysis of the distance between our from-scratch
HVF VMM and Parallels Desktop's engine, and a phased plan to close what is
realistically closable. Grounded in a research pass on Parallels' architecture,
the Windows-ARM64 guest-driver landscape, and Apple-Silicon VM GPU options.

## Where we are (ground truth)
A QEMU-independent VMM on Apple Hypervisor.framework that boots Windows 11 ARM64
to a networked, multi-core desktop. Devices: NVMe, xHCI (HID kbd + absolute
pointer), virtio-blk, virtio-net + userspace NAT, and **ramfb — a fixed-geometry
(800x600), CPU-drawn, UNACCELERATED framebuffer** (Windows Basic Display Adapter
software-renders into it). No GPU accel, audio, clipboard, shared folders,
dynamic resolution, USB passthrough, guest agent, or snapshots.

On a Retina Mac (e.g. 3024x1964 @2x) a fixed 800x600 software framebuffer is the
single worst part of the experience — this is the real "not smooth" problem, more
than raw 3D.

## What Parallels actually does (the moat)
- **Paravirtual GPU, fully custom.** The guest sees "Parallels Display Adapter
  (WDDM)" (PCI 1AB8:4005) — a full WDDM driver (kernel + user-mode D3D DDI)
  shipped by **Parallels Tools**. Without Tools you get only a basic framebuffer
  (i.e. our level). The host translates guest **DirectX → Apple Metal**.
- **Capability: DirectX 11.1 (feature level 11_1) + OpenGL 3.3/4.1. NO DirectX 12,
  no guest Vulkan.** (DX12 is a years-old unfulfilled request; UTM/QEMU can't do
  Windows-ARM 3D at all.)
- **Integration rides "Toolgate"** (PCI 1AB8:4000): one proprietary host↔guest
  channel carrying clipboard (text/images/files), shared folders, drag-drop,
  dynamic resolution/HiDPI, absolute mouse, time sync, guest shutdown. Plus
  custom paravirtual audio (prl_sound) and a memory balloon (1AB8:4006).
- **Key truth:** every piece of the Windows 3D path is closed, Windows-guest-
  specific, and needs a signed WDDM driver Parallels spent years building. There
  is **no open component to reuse for Windows guest 3D.**

## The three honest tiers of the gap

### Tier 0 — already at parity (boot/IO)
NVMe (inbox stornvme), xHCI (inbox USB), networking (our virtio-net is *more*
standard than Parallels' branded NIC). We're essentially even here.

### Tier A — CLOSABLE and high-impact: 2D display done right
**virtio-gpu (2D scanout).** The guest-driver problem is ALREADY SOLVED for
Windows ARM64: the **signed `viogpudo` WDDM Display-Only Driver ships in the
official virtio-win ISO (`viogpudo\w11\ARM64`)** — we inject it exactly like we
inject netkvm today. It gives:
- **Dynamic resolution via EDID** → run at native Retina resolution / any window
  size, not fixed 800x600.
- **Hardware cursor** (separate cursor plane — no full-frame redraw to move the
  mouse).
- **Dirty-rectangle flushes** (TRANSFER_TO_HOST_2D + RESOURCE_FLUSH of only the
  changed region) instead of blitting the whole framebuffer.
- A **real WDDM display adapter** instead of the EFI/ramfb framebuffer, so
  monitor detection, multi-scanout, and mode-setting all work.

Rendering still runs on Windows' CPU Basic Render Driver — **but that is true
under QEMU/UTM too**, so this reaches the ceiling of what any Windows-ARM guest
display can do anywhere short of a proprietary WDDM 3D driver. It is the single
biggest smoothness/usability upgrade available to us and it is achievable
(~1–2 weeks: two virtqueues, a resource table, EDID + config-interrupt resize,
dirty-rect blits — reusing our existing PCIe/virtqueue infra and the ramfb blit
path). **Design it so VIRGL/Venus capsets + blob resources can be bolted on later
without reworking the 2D path.**

### Tier B — CLOSABLE integration: the Toolgate pattern
Copy the *pattern*, not the device: add ONE simple host↔guest channel
(virtio-vsock or virtio-serial) + a small guest agent, and layer our own opcode
protocol on it. That single device unlocks **shared clipboard, shared folders /
drag-drop, dynamic-resolution coordination, absolute-mouse polish, time sync,
graceful shutdown**. Needs a guest agent we write (Windows service) — moderate
effort, but one device model buys many features. Prior art to reuse:
`crates/bridgevm-agentd`, `bridgevm-agent-protocol`.

### Tier C — the real 3D: only reachable for LINUX guests
- **Windows-ARM64 guest 3D is a dead end right now** — not because of our engine,
  but because no working guest driver exists: viogpu3d (virgl GL) is stalled,
  x64-oriented, and crashes on ARM64; the successor Venus/Vulkan WDDM driver is
  experimental, unmerged, and unproven on ARM64. Nothing to integrate. Track it;
  adopt if it matures. This is the wall Parallels only cleared by writing their
  own proprietary WDDM driver — out of scope for us near-term.
- **Linux guest 3D is genuinely reachable** from a from-scratch HVF VMM: a
  virtio-gpu device with the VIRGL/Venus capsets, host-side **libvirglrenderer**
  (builds on macOS) rendering to **MoltenVK** or Mesa's newer **KosmicKrisp**
  (Vulkan-on-Metal). **libkrun proves this exact architecture on Apple Silicon.**
  So "real GPU acceleration via a hand-rolled engine" is feasible — with a Linux
  guest, not Windows.

### Also missing (medium): audio, balloon, USB passthrough, snapshots/save-state
- **Audio**: easiest on Windows via an in-box-driver device (ICH6/HDA) so no
  driver injection is needed; virtio-sound needs an injected driver.
- **Memory balloon**: dynamic RAM reclaim (virtio-balloon; Windows needs the
  virtio-win balloon driver).
- **USB passthrough / snapshots / save-state**: real but lower priority than
  display + integration.

## Roadmap (impact × achievability, highest first)
1. **virtio-gpu 2D + viogpudo injection (Tier A).** Native-resolution resizable
   HiDPI desktop, hardware cursor, dirty-rect flushes. THE biggest win; achievable
   now; ceiling for Windows-ARM display. Start here.
2. **Guest-agent channel + clipboard/shared-folders/resolution (Tier B).**
   virtio-vsock/serial + a small Windows agent; reuse bridgevm-agent-protocol.
3. **Audio (in-box HDA).** Round out the "feels like a real PC" experience.
4. **Linux-guest 3D via virtio-gpu VIRGL/Venus + libvirglrenderer→MoltenVK/
   KosmicKrisp (Tier C).** The genuine GPU-acceleration story — Linux first.
5. **Watch/adopt the Venus WDDM Windows-ARM driver** for Windows guest 3D if it
   becomes real. Balloon, USB passthrough, snapshots as follow-ons.

## Honest verdict
- We will **not** match Parallels' Windows DirectX-over-Metal near-term — it's a
  years-long proprietary WDDM effort and the open Windows-ARM 3D driver doesn't
  exist. Don't budget for Windows guest 3D.
- We **can** reach the best-available Windows-ARM *display* (virtio-gpu 2D:
  native Retina resolution, HW cursor, dirty-rect) and real *integration*
  (clipboard/folders/resolution via a guest agent) — that closes most of the
  daily-use gap and directly fixes "not smooth."
- We **can** deliver real *3D GPU acceleration for Linux guests* (venus→Metal),
  where a from-scratch, QEMU-independent, Apple-Silicon-native engine can actually
  differentiate.

_Research pass 2026-07-05 (Parallels GPU/device/integration architecture,
Windows-ARM64 guest-driver landscape, Apple-Silicon VM GPU options)._
