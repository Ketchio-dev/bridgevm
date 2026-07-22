# BridgeVM documentation

This index separates current product guidance from active engineering plans and
dated evidence. If a dated bring-up note conflicts with `README.md`,
`STATUS.md`, or an active plan, the current documents win.

Last reviewed: 2026-07-22.

## Start here

- [Project overview](../README.md) — engine choice, quick start, and release gates.
- [Current status](../STATUS.md) — concise proven boundary and remaining walls.
- [Contributing](contributing/README.md) — development and verification workflow.
- [Development system](development-system.md) — gate IDs, evidence ladder, work
  packets, risk lanes, and definition of done.
- [Security model](security/model.md) — trust boundaries and security expectations.

## Engine guides

- [Compatibility Engine](compatibility-mode/README.md) — QEMU-backed planning and launch.
- [Apple VZ Engine](fast-mode/README.md) — supported Linux/macOS Arm fast path.
- [Windows 11 Arm](windows-arm/README.md) — custom HVF direction and live evidence index.
- [Guest Tools protocol](guest-tools/protocol.md) — host/guest transport contract.

Some engine guides still contain detailed implementation histories. Their first
sections describe the current contract; dated claims deeper in those files
should be read as engineering context until they are split into the history
set.

## Active Windows HVF plans and decisions

- [Windows completion plan](hvf-windows-install-completion-plan.md) — authoritative
  remaining release work and acceptance gates.
- [Competitive architecture and risk policy](hvf-competitive-architecture-and-risk-policy.md)
  — QEMU, VMware, and Parallels findings; balanced/aggressive performance policy;
  vTPM/Secure Boot lifecycle constraints.
- [Windows engine strategy](hvf-windows-engine-strategy.md) — no-QEMU product boundary.
- [Windows platform contract gap](hvf-windows-platform-contract-gap.md) — firmware and
  device-contract comparison.
- [Windows v1 suspend decision](hvf-windows-v1-suspend-decision.md) — durable suspend is
  explicitly outside v1 until its state contract is proven.
- [Windows 3D plan](hvf-p3-windows-3d-plan.md) and
  [3D engine plan](hvf-3d-engine-plan.md) — graphics ladder.
- [Performance optimization plan](hvf-perf-optimization-plan.md),
  [GPU thread design](hvf-gpu-thread-design-20260721.md), and
  [graphics integration gap plan](hvf-graphics-integration-gap-plan.md).

## Historical evidence and wall resolutions

These are immutable or near-immutable snapshots of what a specific run proved.
They are valuable evidence, but they are not current onboarding material:

- `hvf-*-20260713.md` through `hvf-*-20260721.md` — dated 3D, fence, scanout,
  WDDM, DXVK, and real-title investigations.
- [Windows Arm evidence](windows-arm/evidence/) — dated storage, reboot, guest-agent,
  WDK, and driver-bind receipts.
- [KD serial bring-up](hvf-kd-serial-bringup.md) — kernel-debugging procedure and notes.
- [Previous root README](archive/README-before-20260722.md) — exact 399-line
  onboarding/status document preserved before the 2026-07-22 information
  architecture rewrite.
- [Previous root STATUS](archive/STATUS-before-20260722.md) — exact 974-line
  accumulated status log preserved before it was replaced by the concise gate
  report.

The date in a filename identifies the observation date, not a promise that the
claim still describes the latest code.

## Reference

- [Phase 0 architecture](architecture/phase-0.md) — original scaffold design; historical
  architecture reference, not current project status.
- [QEMU virt AArch64 GICv3 DTS](reference/qemu-virt-aarch64-gicv3.dts) — platform
  contract reference.
- [Root long-form plan](../PLAN.md) — product thesis and accumulated roadmap context.

## Document status convention

New or substantially revised documents should declare one of these states near
the title:

- **Current** — describes the presently supported or proven product boundary.
- **Active plan** — accepted work not yet fully implemented or evidenced.
- **Decision** — an adopted architectural or product constraint.
- **Historical evidence** — a dated result that must not silently become a promise.
- **Reference** — stable background material.

Use absolute dates for live evidence, distinguish deterministic tests from live
guest proof, and link a superseding current document from obsolete plans rather
than deleting useful history.

The machine-readable classification is
[`document-manifest.tsv`](document-manifest.tsv). Validate it with
`bash scripts/check-documentation-system.sh`; the check fails when a Markdown
document is missing from the manifest.
