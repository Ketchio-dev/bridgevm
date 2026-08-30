# BridgeVM Windows HVF 3D architecture

Document status: **Active plan**

Last revised: 2026-08-13

This document describes the architecture and engineering boundaries of
BridgeVM's accelerated graphics path. Current capability state and live
measurements belong in the
[Windows capability matrix](../windows-arm/capability-matrix.md), not in this
active plan.

## Goal

Provide a hardware-accelerated graphics path for Windows 11 Arm guests running
on BridgeVM's custom Hypervisor.framework VMM while preserving a simple,
recoverable 2D path.

BridgeVM owns the virtualization-specific integration:

- virtio-gpu PCI transport and queues;
- 3D contexts, resources, fences, and reset lifecycle;
- shared-memory BAR handling and host mappings;
- host renderer process integration;
- scanout/presentation into the macOS app;
- Windows install/injection orchestration and evidence collection.

Graphics API translation and shader compilation are intentionally delegated to
specialized third-party components under their own licenses rather than being
implemented inside the VMM.

## Data path

```text
Windows application
        |
        |  Vulkan directly, or a guest-side graphics translation layer
        v
Guest Vulkan / display components
        |
        |  virtio-gpu commands, resources, shared memory, fences
        v
BridgeVM virtio-gpu device model
        |
        |  renderer protocol
        v
Host renderer process / library
        |
        |  host Vulkan / Metal-compatible presentation
        v
macOS + Apple GPU
```

The exact guest and host renderer components may change as the experimental
stack evolves. The virtio-gpu contract and BridgeVM-owned lifecycle should stay
isolated from those implementation choices.

## BridgeVM-owned responsibilities

### virtio-gpu 3D device model

The VMM is responsible for:

- capability negotiation;
- context create/destroy;
- resource attach/detach;
- 3D submission;
- blob-resource lifecycle;
- map/unmap handling;
- scanout association;
- fence completion and interrupt delivery;
- complete teardown on device reset and VM exit.

The 2D path remains a recovery boundary. Enabling 3D must not make the VM disk,
UEFI state, or security identity depend on the accelerated renderer being
healthy.

### Shared-memory mapping

Host-visible GPU resources cross one of the most sensitive VMM boundaries.
Mapping code must treat alignment, lifetime, permissions, reset, and teardown as
correctness constraints rather than performance details.

Required invariants:

1. guest-requested offsets and sizes are range checked;
2. host mappings are page aligned and bounded to the declared shared-memory
   aperture;
3. a resource cannot be freed while an active mapping still owns it;
4. reset/teardown removes every mapping before backing storage is released;
5. stale guest BAR programming cannot leave a mapping active at an old guest
   physical address;
6. mapping failure returns an error instead of falling back to an unsafe alias.

### Fences and completion

Fence ordering must be observable and deterministic. Completion callbacks from a
renderer thread/process are converted into device completions and MSI-X delivery
without allowing a stale context or reset generation to signal a new guest.

A lost fence is a hang; a duplicated or cross-generation fence is corruption.
Both are treated as correctness bugs.

### Presentation

Presentation is separate from command execution. The renderer can finish work
without forcing a synchronous full-frame CPU readback.

The preferred path is:

```text
guest render -> host-visible resource -> asynchronous scanout/present -> macOS
```

Readback remains useful for evidence, screenshots, debugging, and fallback, but
it must not accidentally become the steady-state frame path.

## Guest-side responsibilities

Windows needs a kernel/display path that can expose the negotiated virtio-gpu
features to user-mode graphics components. Driver installation is treated as an
explicit package lifecycle:

- INF/SYS/CAT identity stays together;
- test-signed packages are activated only through the test-signing flow;
- a package is verified after bind rather than assumed healthy because install
  returned success;
- stale DriverStore generations are removed deliberately;
- package identity is recorded in live evidence.

BridgeVM does not redistribute Windows itself. Windows media is supplied by the
user.

## Third-party component boundary

The graphics stack can include components such as:

| Component family | Role | License boundary |
| --- | --- | --- |
| Mesa graphics components | guest/host Vulkan implementation pieces | retain Mesa license notices |
| virglrenderer | host renderer/protocol implementation | retain upstream license notices |
| MoltenVK | Vulkan-to-Metal implementation where used | retain Apache-2.0 license/NOTICE obligations |
| DXVK | optional D3D9/10/11-to-Vulkan guest translation | retain zlib license notice |
| vkd3d-proton | optional D3D12-to-Vulkan guest translation | LGPL obligations apply if distributed |
| virtio Windows driver components | guest kernel/display/network packages | retain each package's license and notices |

This table describes component roles only. The authoritative redistribution
inventory is [`../THIRD-PARTY-NOTICES.md`](../../THIRD-PARTY-NOTICES.md), and the
packaging gates decide what is actually shipped.

Code from a third-party project must not be copied into BridgeVM merely because
its behavior is useful as a reference. Use documented interfaces/specifications,
keep third-party code in its licensed component boundary, and preserve required
notices for anything redistributed.

## Validation ladder

Graphics work moves through a strict ladder. A later rung does not retroactively
prove an earlier one on a different build.

### 1. Deterministic model tests

Cover:

- context/resource lifecycle;
- map/unmap boundaries;
- fence ordering;
- reset teardown;
- malformed descriptor/command handling;
- PCI/BAR and capability state transitions.

### 2. Host renderer tests

Prove that the pinned renderer stack builds, links, exposes the expected capset,
and can process representative commands on the target Mac.

### 3. Guest API smoke

Prove that the Windows guest loads the intended driver/ICD and performs actual
3D submissions. A successful DLL load by itself is insufficient.

### 4. Real workload

Run a real application with identity checks for the executable, driver and
renderer path. Record frame samples, duration, renderer identity, failures, and
clean shutdown.

### 5. Soak and recovery

Repeat workload start/stop, resize, reset, VM restart, and renderer failure
recovery. Check for leaked mappings, stale fences, hung queues, corrupted state,
and guest-driver instability.

## Performance measurements

Average FPS alone is not enough for the next stage of the project. Useful
measurements include:

- frame-time p50/p95/p99;
- 1% and 0.1% low frame rate;
- command submission/fence latency;
- synchronous readback time;
- shared-memory map churn;
- renderer CPU usage;
- host memory growth during long runs;
- resize/full-screen transition behavior;
- application startup and shader-compilation stalls.

Optimization work must keep the same correctness gates. A faster result obtained
by weakening identity, timeout, renderer, or workload requirements is not a
valid performance improvement.

## Failure policy

The accelerated path is experimental and must remain recoverable.

- Renderer failure must not corrupt VM media.
- Device reset must release all graphics resources from the old generation.
- A 3D failure should leave a diagnosable path back to the non-accelerated
  display mode where possible.
- Guest-driver setup failure must surface as an install/runtime error rather than
  silently claiming 3D is active.
- Security state is independent of the selected graphics policy.

## Current direction

The short-term graphics work is no longer "prove that any 3D command can cross
the VMM." The current capability registry already contains live Vulkan and
D3D11 evidence. The next value is broader compatibility, longer soak coverage,
better frame pacing, cleaner distribution of the guest driver stack, and simpler
failure recovery for preview users.
