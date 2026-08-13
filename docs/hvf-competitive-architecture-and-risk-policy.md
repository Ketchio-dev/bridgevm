# Windows HVF architecture and risk policy

Document status: **Decision**

Adopted: 2026-07-22  
Last revised: 2026-08-13

This document defines the product constraints for BridgeVM's Windows 11 Arm
engine. It describes BridgeVM's own runtime, guest-visible contract, security
lifecycle, and reversible performance policies. It is not a competitive-product
analysis and should not be used as one.

## Product boundary

The Windows HVF engine is a BridgeVM-owned VMM built directly on Apple's
Hypervisor.framework. Its guest-visible platform is a **QEMU `virt`-compatible
contract with documented deviations** so standard AArch64 firmware and guest
software have a stable machine shape.

Compatibility is a protocol and machine-contract choice. BridgeVM owns the host
runtime, device models, lifecycle, evidence surfaces, and product behavior.
Guest-visible deviations are recorded in
[`machine-contract/qemu-virt-deviations.json`](machine-contract/qemu-virt-deviations.json)
instead of being hidden behind informal compatibility claims.

The Windows engine currently includes:

- UEFI firmware and persistent variable storage;
- PCIe/ECAM, NVMe, xHCI, virtio networking, audio, and virtio-gpu devices;
- guest-agent transport for lifecycle and integration features;
- TPM 2.0, Secure Boot, measured-boot, and recovery state;
- typed process-recreate runtime supervision;
- experimental Vulkan and D3D11-compatible 3D paths.

The machine contract is intentionally narrower than a promise that arbitrary PC
hardware or arbitrary Windows software is supported.

## Design rules

### 1. Guest-visible contracts are explicit

A guest-visible behavior belongs in one of three places:

1. a stable protocol or platform contract;
2. a documented BridgeVM deviation;
3. an experimental capability guarded by an explicit product state.

Do not rely on undocumented behavior merely because one guest image happens to
accept it.

### 2. Stateful security changes fail closed

The engine never silently replaces or resets identity-bearing state when its
provenance is missing.

This applies to:

- vTPM state and its encryption key;
- Secure Boot variables;
- BitLocker-sensitive identity and PCR state;
- UEFI variables;
- disk/vars snapshot pairs;
- migration and recovery packages.

If BridgeVM cannot establish that state is the expected state for the VM, launch
or mutation fails with a diagnostic instead of inventing a replacement.

### 3. Performance switches do not rewrite VM identity

BridgeVM exposes two media-independent launch policies:

- `balanced`: conservative renderer/presentation behavior and the recovery lane;
- `aggressive`: the product 3D lane, enabling the direct renderer,
  asynchronous scanout, IOSurface-backed presentation, and the current
  high-performance I/O choices.

`--performance-risk aggressive` requires the 3D path. Every run records the
resolved policy in evidence. Moving from `aggressive` back to `balanced` must not
rewrite the guest disk, UEFI variables, driver package, vTPM state, or recovery
identity.

A performance optimization is acceptable only when all of the following hold:

1. it has a bounded rollback path;
2. enabling it does not silently mutate durable security state;
3. a run records enough information to identify the active policy;
4. the relevant correctness and live-workload gates still apply.

### 4. Evidence outranks architectural intent

A design note can explain why a mechanism exists, but it does not prove the
mechanism works in a real guest. Capability promotion follows the evidence
hierarchy in [`AGENTS.md`](../AGENTS.md): live receipts, live single runs,
automated tests, then static reasoning.

No threshold is lowered to make a feature pass.

## vTPM and Secure Boot lifecycle

The product vTPM path has three layers:

1. the VMM exposes the Windows-visible TPM 2.0 TIS/PPI/ACPI surfaces and a
   measured-boot event-log region;
2. the runtime supervises a per-VM `swtpm` process and supplies the state key over
   an inherited file descriptor rather than argv or a key file;
3. the app stores a device-local 256-bit key per stable VM identity in macOS
   Keychain and manages recovery/clone/reset semantics explicitly.

The active-state rules are:

- a brand-new empty vTPM state may create a key;
- existing non-empty state with no matching key is refused;
- moving a VM on the same Mac preserves its stable identity;
- recovery restore requires authenticated package metadata and the expected VM
  identity;
- cloning creates a fresh VM identity and fresh TPM state;
- confirmed reset archives prior state before rotating the active identity.

The security implementation must never make a VM appear healthy by discarding
state that could be required for guest recovery.

## Storage and snapshot boundary

The supported v1 persistence model is powered-off snapshot/restore of the disk
and UEFI-vars pair. Running-state suspend is intentionally outside the v1
contract until CPU, interrupt-controller, device, renderer, and security state
can be serialized and restored as one coherent operation.

Snapshot and restore operations therefore require:

- an explicit powered-off state;
- writer exclusion for the live disk and vars pair;
- bounded space checks;
- content digests and an atomic pair manifest;
- refusal before live-file mutation when verification fails.

See [`windows-arm/snapshot-scope-v1.md`](windows-arm/snapshot-scope-v1.md).

## Graphics risk boundary

3D is an experimental capability, not a reason to weaken VM security or storage
correctness.

The graphics path may select more aggressive scheduling, presentation, or
readback behavior, but it must preserve:

- deterministic teardown;
- bounded resource ownership;
- device/reset lifecycle correctness;
- guest-driver identity checks in release evidence;
- a 2D/recovery lane when the accelerated path is unsuitable.

A synthetic draw proves plumbing. A real title receipt proves only the measured
workload on the stated build. Neither is a universal compatibility claim.

## Distribution boundary

BridgeVM's Engineering Preview can be built and distributed with ad-hoc macOS
signing. Developer ID signing/notarization is a distribution convenience and
trust improvement, not a prerequisite for the core VMM to run on a developer
Mac.

Windows media remains user-supplied. Experimental Windows driver packages may
require test-signing mode; production driver signing is a separate distribution
milestone and is not conflated with ownership of the Windows ISO.

## Third-party components

BridgeVM uses third-party firmware, runtime libraries, and guest components only
under their respective licenses. Attribution and redistribution obligations are
tracked in [`../THIRD-PARTY-NOTICES.md`](../THIRD-PARTY-NOTICES.md).

Architecture documents should describe component roles and interfaces, not make
claims that BridgeVM source code was copied from another product. Required
license notices and upstream provenance must remain intact even when historical
competitive-research prose is removed.
