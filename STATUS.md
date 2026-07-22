# BridgeVM current status

Document status: **Current**
Last reviewed: **2026-07-22**

This is the short product boundary. The previous 974-line status log is
preserved unchanged as
[STATUS-before-20260722.md](docs/archive/STATUS-before-20260722.md), and the
documentation index links the dated evidence behind the summary below.

## Executive judgment

BridgeVM is a substantial engineering preview, not a Phase 0 scaffold and not
yet a public-production virtualization product.

- The QEMU Compatibility Engine has real launch supervision, storage,
  networking, snapshots, diagnostics, and guest-tools plumbing.
- The Apple VZ Engine has a real, signed Linux Arm launch/display path for its
  currently supported kernel plus raw-disk shape.
- The custom Windows HVF Engine boots an installed Windows 11 Arm desktop
  without QEMU and has working persistent NVMe, SMP, display/input, networking,
  audio, guest-agent control, restart/reset, and experimental accelerated 3D.
- Windows HVF release readiness remains blocked by the vTPM/Secure Boot
  lifecycle, production driver signing, fresh same-boot guest receipts, and
  distribution signing/notarization.

## Engine matrix

| Engine | Proven today | Important open boundary |
| --- | --- | --- |
| Compatibility / QEMU | Safe plans; explicit image creation and inspection; supervised launch/stop; NAT/forwards; snapshot and diagnostics paths | Privileged macOS vmnet modes, full guest/GUI coverage, public packaging |
| Apple VZ | Signed Linux Arm runner; raw disk plus direct-kernel boot; display/control socket and framebuffer export | Wider boot media/disk formats, full desktop integration, live CPU/RAM reapply, release packaging |
| Windows HVF | Installed Windows desktop; 4 vCPUs; writable NVMe/UEFI vars; RAMFB/input; virtio network/audio/agent; clean shutdown/restart; Venus/VirGL experimental 3D and real PPSSPP Vulkan evidence; local TPM TIS/PPI/TPM2 log and Keychain-to-swtpm encrypted-state path | Windows TPM/measured-boot receipt; Secure Boot key lifecycle; bundled swtpm; fresh WDK package and production signature; current real-title receipt; public signing/notarization |

## Windows HVF evidence boundary

The installed-Windows path has live evidence for:

- clean system-off, NVMe flush/writeback, and post-exit reopen;
- in-process Windows restart with BridgeVM device, guest RAM, vCPU, and Apple
  in-kernel GIC reset;
- resident BVAGENT command execution and bounded chunked output;
- guest-requested shutdown and first-boot disk-growth actions;
- a bound experimental Windows ARM64 display stack;
- host-visible Vulkan rendering and PPSSPP 1.20.4 running with its native
  Vulkan backend;
- deferred scanout/readback instrumentation that identifies synchronous
  GPU-to-CPU readback as a major remaining display cost.

Those observations are dated evidence, not a promise that an arbitrary Windows
image or game works. The relevant receipts are indexed under
[historical evidence](docs/README.md#historical-evidence-and-wall-resolutions).

## Adopted performance policy

The installed-Windows launcher exposes two reversible policies:

- `balanced` is the CLI recovery/default lane.
- `aggressive` enables the direct renderer, asynchronous scanout, IOSurface
  scanout, and zero artificial readback interval for 3D; the macOS app selects
  this lane.

Aggressive mode is acceptable because it is one-switch reversible, does not
rewrite VM media merely by being selected, and is recorded in run evidence.
It does not weaken security-state handling. See the
[architecture and risk policy](docs/hvf-competitive-architecture-and-risk-policy.md).

## Remaining release walls

| Gate | State | Difficulty judgment | Completion evidence |
| --- | --- | --- | --- |
| `SEC-TPM-FRONTEND` | `IN_PROGRESS` | Bounded device-model work | Windows enumerates ACPI `MSFT0101`; TIS commands and PPI actions work |
| `SEC-TPM-LIFECYCLE` | `IN_PROGRESS` | Hard security/lifecycle work | Implemented encrypted per-VM state + Keychain/FD custody; still needs bundled runtime and tested move/clone/restore/reset UI |
| `SEC-SB-MEASURED` | `IN_PROGRESS` | Hard cross-layer work | Pinned Secure Boot + TPM2 EDK2 and fail-closed Microsoft-only PK/KEK/db/dbx provisioning are locally proven; guest proof from `Confirm-SecureBootUEFI`, PCR 7, event log, and recovery/migration workflows remains |
| `GPU-WDK-SIGN` | `EXTERNAL` | Externally gated | Fresh ARM64 WDK build, catalog, trusted signature, and clean bind |
| `GPU-LIVE-RECEIPT` | `OPEN` | Live-machine gated | Same-boot bind/title/crash-free/performance evidence from the packaged app |
| `DIST-MACOS` | `EXTERNAL` | Externally gated | Developer ID, hardened runtime, notarization, clean-machine install and launch |

The TPM register model and ACPI plumbing are comparatively straightforward.
Correct identity, migration, BitLocker recovery, signing, and reproducible live
evidence are not “easy last steps”; they are the release-quality work.

Current `SEC-TPM-FRONTEND` evidence is E2 only: five TIS localities, command
FIFO, the 1 KiB PPI mailbox, PPI 1.3/reset-mitigation `_DSM`, fixed MMIO
dispatch, optional ACPI `TPM0/MSFT0101`, and the revision-4 TPM2 table with a
loader-relocated 64 KiB `etc/tpm/log` area are unit proven. The installed-boot
launcher now owns a fail-closed swtpm process/socket lifecycle and preserves its
per-VM state directory. The app product configuration supplies a per-VM
256-bit `WhenUnlockedThisDeviceOnly` Keychain key through a one-shot inherited
FD; swtpm encrypts state with AES-256-CBC plus encrypt-then-MAC, and an existing
state directory with a missing key is never assigned a silent replacement key.
The repository smoke proves exact 32-byte FD delivery, socket/process cleanup,
and persistent state without putting the key in argv or a disk keyfile.
Firmware-populated measured-boot events, a bundled/signed swtpm runtime,
move/clone/restore/reset UI, and real Windows enumeration remain open, so both
security gates stay `IN_PROGRESS`.

`SEC-SB-MEASURED` also has deterministic local evidence. The default firmware
is a reproducible EDK2 build pinned to commit
`b03a21a63e3bd001f52c527e5a57feddb53a690b` with Secure Boot and TPM2 enabled;
its 3 MiB code volume is pinned by SHA-256. Fresh-install finalization validates
Microsoft's ARM64 `secureboot_objects` v1.6.5 payloads and source provenance,
then writes `dbx`, `db`, `KEK`, and `PK` in that order. Exact state is
idempotent; partial, duplicate, or conflicting state fails without mutation.
The packaged path includes the policy, build receipt, and license notices.
This is E2—not a live assertion that Windows reports Secure Boot or that PCR 7,
BitLocker recovery, and VM migration are correct.

Windows HVF durable suspend is intentionally outside v1. The experimental
checkpoint path must not be advertised as suspend; see the
[v1 suspend decision](docs/hvf-windows-v1-suspend-decision.md).

## How to verify the repository

Deterministic local checks:

```sh
cargo test --workspace
swift test --package-path apps/macos
tests/integration/product-gates-report.sh
```

The Swift command requires a matching Xcode/Command Line Tools and SDK pair.
Live HVF, Apple VZ, Windows, entitlement, signing, and notarization claims need
their dedicated machine/guest receipts; deterministic tests alone do not clear
those gates.

## Sources of truth

- [README](README.md) — onboarding and engine selection.
- [Documentation index](docs/README.md) — current, active, historical, and
  reference documents.
- [Windows completion plan](docs/hvf-windows-install-completion-plan.md) —
  authoritative remaining implementation sequence.
- [Long-form plan](PLAN.md) — preserved product thesis and roadmap history.

When status changes, update this file and the authoritative active plan. Add a
dated evidence document for live results instead of growing this page into
another chronological log.
