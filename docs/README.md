# BridgeVM documentation

Document status: **Current**

Last reviewed: 2026-08-13

This index separates current product documentation, active engineering plans,
and dated evidence. If a historical note conflicts with the root `README.md`,
`STATUS.md`, or the machine-readable capability registry, the current documents
win.

## Start here

- [Project overview](../README.md) — what BridgeVM is, current capability state,
  quick start, preview distribution, and repository map.
- [Current status](../STATUS.md) — concise evidence-backed product boundary.
- [Windows capability matrix](windows-arm/capability-matrix.md) — fixed criteria,
  thresholds, measurements, and evidence paths.
- [Windows 11 Arm guide](windows-arm/README.md) — Windows HVF engine guide and
  evidence entry point.
- [Security model](security/model.md) — trust boundaries and fail-closed rules.
- [Kernel-policy provenance](security/kernel-policy-provenance.md) — signed
  Windows driver package trust, rotation, revocation, and snapshot boundary.
- [Contributing](contributing/README.md) — development and verification workflow.
- [Development system](development-system.md) — evidence levels, work packets,
  live gates, and definition of done.

## Engine guides

### Windows HVF

- [Windows 11 Arm guide](windows-arm/README.md)
- [Machine contract](machine-contract/qemu-virt.md)
- [Machine-contract deviations](machine-contract/qemu-virt-deviations.json)
- [Windows architecture and risk policy](hvf-competitive-architecture-and-risk-policy.md)
- [Windows engine strategy](hvf-windows-engine-strategy.md)
- [Snapshot scope](windows-arm/snapshot-scope-v1.md)
- [SMCCC TRNG / PSCI contract](windows-arm/smccc-trng-psci-contract.md)
- [v1 suspend decision](hvf-windows-v1-suspend-decision.md)

### Apple VZ

- [Apple VZ engine guide](fast-mode/README.md)

### Compatibility Engine

- [QEMU compatibility guide](compatibility-mode/README.md)

### Guest integration

- [Guest tools protocol](guest-tools/protocol.md)

## Active engineering plans

These files describe engineering direction, not current product promises:

- [Windows completion plan](hvf-windows-install-completion-plan.md)
- [Windows 3D plan](hvf-p3-windows-3d-plan.md)
- [3D architecture](hvf-3d-engine-plan.md)
- [Graphics and integration roadmap](hvf-graphics-integration-gap-plan.md)
- [Performance optimization plan](hvf-perf-optimization-plan.md)
- [GPU thread design](hvf-gpu-thread-design-20260721.md)
- [HVF refactor extraction plan](hvf-lib-refactor-extraction-plan.md)

Plans can contain experiments that were later falsified or superseded. Use the
capability registry and dated receipts to decide what is proven.

## Live testing

- [Apple-silicon live gates](testing/apple-silicon-live-gates.md) — trusted-host
  queue, sealed inputs, receipts, and the boundary between hosted CI and real
  virtualization evidence.

Hosted CI proves deterministic properties. It does not prove a real Windows
boot, Hypervisor.framework behavior on a physical Apple-silicon host, or a
rendered guest frame.

## Current Windows evidence

The capability matrix links the exact evidence used for each current criterion.
The most useful entry points include:

- [in-app install through 3D desktop](windows-arm/evidence/app-install-to-3d-desktop-20260730.md)
- [Vulkan/D3D title measurement](windows-arm/evidence/a2-a3-title-fps-measurement-20260801.md)
- [D3D11 3/3 title receipt](windows-arm/evidence/a3-d3d11-title-fps-3of3-20260812.md)
- [cold-boot 10/10 receipt](windows-arm/evidence/a1-p1-boot-gate-10of10-20260810.md)
- [TPM PPI clear](windows-arm/evidence/vtpm-windows-ppi-clear-20260722.md)
- [Secure Boot / measured boot](windows-arm/evidence/sb-guest-proof-20260723.md)
- [second-Mac vTPM migration](windows-arm/evidence/second-mac-migration-20260723.md)
- [snapshot pair](windows-arm/evidence/a19-snapshot-pair-20260804.md)
- [snapshot restore boot](windows-arm/evidence/a19-restore-boots-20260805.md)

A dated receipt says what one exact build and fixture proved. It does not become
a permanent compatibility claim for every later build.

## Historical evidence

Bring-up logs are intentionally retained because failed experiments and old
measurements are useful engineering records. They are **not** onboarding
material and they do not override current product state.

Examples include:

- dated `hvf-*-202607*.md` graphics, fence, scanout, WDDM, and title
  investigations;
- older Windows evidence under `windows-arm/evidence/`;
- [previous root README](archive/README-before-20260722.md);
- [previous root STATUS](archive/STATUS-before-20260722.md).

Historical files may name tools, upstream components, or products that were
part of an investigation at that time. Current architecture and licensing are
defined by the present source tree, the current docs above, `LICENSE`, and
`THIRD-PARTY-NOTICES.md`.

## Reference material

- [Phase 0 architecture](architecture/phase-0.md) — early architecture history.
- [QEMU virt AArch64 GICv3 DTS](reference/qemu-virt-aarch64-gicv3.dts) — machine
  contract reference data.
- [Windows platform contract gap](hvf-windows-platform-contract-gap.md) — earlier
  platform bring-up reference.
- [KD serial bring-up](hvf-kd-serial-bringup.md) — kernel-debugging procedure.

Reference data exists to make interfaces and experiments reproducible. It is not
a statement that BridgeVM source code was copied from the referenced project.
Third-party source actually redistributed by BridgeVM remains governed by its
own license and attribution requirements.

## Document status convention

New or substantially revised Markdown documents should be classified in
[`document-manifest.tsv`](document-manifest.tsv) as one of:

- **Current** — presently supported or proven product boundary;
- **Active plan** — accepted work not yet fully implemented/evidenced;
- **Decision** — adopted architectural or product constraint;
- **Historical evidence** — dated observation that must not silently become a
  current promise;
- **Reference** — stable background or reproducibility material.

Use absolute dates for live evidence and distinguish deterministic tests from
live guest proof.

Validate the documentation system with:

```sh
bash scripts/check-documentation-system.sh
```
