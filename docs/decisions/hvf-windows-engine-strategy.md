# BridgeVM Windows HVF engine strategy

Document status: **Decision**

Last revised: 2026-08-13

## Objective

BridgeVM's Windows HVF engine runs Windows 11 Arm on Apple silicon through a
BridgeVM-owned VMM built directly on Hypervisor.framework.

The engine is intentionally separate from the QEMU Compatibility Engine. QEMU is
still useful when broad compatibility or emulation is the goal, but the Windows
HVF path owns its process lifecycle and virtual devices.

## Guest platform contract

The adopted guest-visible platform is a **QEMU `virt`-compatible contract with
documented deviations**.

That choice gives firmware and operating systems a stable, widely implemented
AArch64 machine shape while allowing BridgeVM to own the host implementation.
Compatibility is defined by guest-observable interfaces, not by duplicating
another VMM's internal process structure or source code.

The contract includes the platform pieces Windows and the firmware rely on,
including:

- AArch64 RAM and firmware layout;
- GICv3 interrupt delivery;
- PCIe ECAM/configuration space;
- `fw_cfg`-compatible firmware data exchange where required by the selected
  firmware;
- ACPI/SMBIOS data presented to the guest;
- PCI devices such as NVMe, xHCI, virtio networking, audio, TPM, and virtio-gpu.

Every intentional guest-visible difference belongs in
[`machine-contract/qemu-virt-deviations.json`](../machine-contract/qemu-virt-deviations.json).

## Host implementation boundary

BridgeVM owns:

- Hypervisor.framework VM/vCPU lifecycle;
- guest physical-memory management;
- PCIe and MMIO dispatch;
- interrupt routing and reset coordination;
- NVMe storage and writeback behavior;
- xHCI input;
- networking and audio integration;
- virtio-gpu 2D/3D device behavior;
- guest-agent transport;
- vTPM process/key lifecycle;
- process-recreate reset handling;
- snapshots and evidence surfaces.

Third-party firmware, rendering libraries, guest drivers, and TPM components stay
inside their respective licensed component boundaries. See
[`../THIRD-PARTY-NOTICES.md`](../../THIRD-PARTY-NOTICES.md).

## Why keep multiple engines

A single virtualization backend is not optimal for every workload.

### Windows HVF

Use when the goal is BridgeVM's custom Windows 11 Arm path and its device,
security, integration, and graphics work.

### Apple VZ

Use for the narrower Linux/macOS Arm path where Apple's
Virtualization.framework is the appropriate product boundary.

### Compatibility Engine

Use when broad QEMU guest support, unusual hardware, or architecture emulation
matters more than the custom Windows runtime.

Keeping these engines explicit avoids hiding materially different security,
compatibility, and lifecycle behavior behind one vague "fast mode" switch.

## Windows HVF runtime layers

```text
BridgeVM SwiftUI app
        |
        v
bridgevm-hvf-runtime / hvf-runner
        |
        v
BridgeVM Hypervisor.framework VMM
        |
        +-- firmware / ACPI / SMBIOS
        +-- PCIe / interrupts
        +-- NVMe
        +-- xHCI input
        +-- virtio networking
        +-- HDA audio
        +-- guest agent
        +-- vTPM / Secure Boot lifecycle
        +-- virtio-gpu 2D / experimental 3D
        |
        v
Windows 11 Arm guest
```

The product app should use typed launch state rather than shell conventions as
its long-term contract. Shell wrappers remain useful for diagnostics and live
engineering gates, but they are not the desired ownership boundary for core
product lifecycle state.

## State and recovery rules

The runtime is responsible for keeping durable state coherent across helper
process recreation.

Durable state includes:

- guest disk;
- UEFI variables;
- vTPM state and Keychain-associated identity;
- powered-off snapshot manifests;
- recovery/migration metadata.

A helper crash or guest reset must not silently create a new identity, discard
pending writes, or attach stale resources from the previous process generation.

## Security strategy

Security features remain fail-closed:

- guest entropy comes from the host CSPRNG;
- missing vTPM key provenance does not create a replacement key for existing
  state;
- Secure Boot and measured-boot claims require guest-visible evidence;
- release builds do not honor development helper overrides;
- snapshot/restore verification completes before live media is mutated.

Graphics performance policy is independent of those rules.

## Graphics strategy

The accelerated graphics path is intentionally layered:

1. BridgeVM owns the virtio-gpu device model, mapping, fences, reset, and
   presentation integration;
2. third-party renderer/graphics components operate behind explicit interfaces;
3. Windows guest packages are installed and verified as versioned packages;
4. real workload receipts are required before capability claims are promoted.

The non-accelerated display path remains a recovery boundary.

See [3D architecture](../plans/hvf-3d-engine-plan.md) and the
[graphics/integration roadmap](../plans/hvf-graphics-integration-gap-plan.md).

## Product sequencing

The current engine has moved beyond initial firmware and desktop bring-up. The
highest-value work now is:

1. keep the final capability/no-regression gate honest at each preview cut;
2. make ad-hoc preview packaging reproducible and license-complete;
3. simplify user-supplied Windows ISO installation;
4. make experimental guest-driver setup and its failure modes explicit;
5. widen GPU/application compatibility and frame-time coverage;
6. improve clean-machine install, recovery, diagnostics, and update UX;
7. move optional production signing/notarization work later without blocking
   technical preview users.

New low-level devices should be added only when they unlock a concrete guest or
product requirement. Breadth for its own sake is not the goal.

## Evidence and source-of-truth rules

Current product wording comes from
[`../capabilities/windows-hvf.json`](../../capabilities/windows-hvf.json).

- `README.md` explains the project to users.
- `STATUS.md` summarizes the current evidence boundary.
- the capability matrix owns per-criterion thresholds and measurements;
- dated evidence files preserve exact observations;
- this strategy document explains architectural decisions only.

Historical research, old comparisons, and bring-up notes do not override those
sources and should not be presented as claims about where BridgeVM source code
came from.
