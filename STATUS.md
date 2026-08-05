# BridgeVM current status

Document status: **Current**
Last reviewed: **2026-08-04**

<!-- BEGIN GENERATED: capability-summary -->
**Product state: Engineering Preview.** Boots an installed Windows 11 Arm desktop on BridgeVM's own Hypervisor.framework VMM with storage, display/input, network, audio, guest agent and experimental 3D. Not release-ready.

Release-blocking criteria proven: **10 / 19**. Open: A1, A2, A3, A11, A14, A15, A16, A17, A18.

- Graphics: Experimental Vulkan path and Experimental D3D11-compatible subset.
- Guest platform: QEMU virt-compatible guest contract with documented deviations.

State reviewed 2026-08-04 at commit `dd273c5`. This block is generated from [`capabilities/windows-hvf.json`](capabilities/windows-hvf.json) by `scripts/render-capability-status.py`.
<!-- END GENERATED: capability-summary -->

Per-criterion state, thresholds and latest measurements are in the
[capability matrix](docs/windows-arm/capability-matrix.md).

> **Graphics reproducibility update (2026-08-01).** The `vkCreateInstance` spin
> that made the dated GPU receipts below unreproducible is **fixed**. The cached
> virtio-gpu BAR2 base was refreshed only from cpu0, but Windows reprograms that
> BAR from a secondary CPU, so `hv_vm_map` installed the Venus ring at an address
> the guest no longer used. Five consecutive runs now create an instance in
> 196-255 ms, DXVK D3D11 reports the real adapter
> (`Virtio-GPU Venus (Apple M4 Max)`) and passes its present smoke, and PPSSPP
> runs with `vulkan_virtio.dll` loaded. Dynamic resize also works now
> (1600x900 / 1920x1080 / 1280x720 all applied live). What is still **not**
> proven is a process-attributed frame rate for a real title; the A2/A3 gates
> remain open.
> See [the vkCreateInstance spin investigation](docs/windows-arm/evidence/a1-a2-a3-vkcreateinstance-spin-20260731.md).

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
  audio, guest-agent control, restart/reset, and experimental accelerated 3D
  whose Vulkan and D3D11 paths now start reliably (see the update above).
- Windows HVF release readiness remains blocked by the vTPM/Secure Boot
  lifecycle, production driver signing, fresh same-boot guest receipts, and
  distribution signing/notarization.

## Engine matrix

| Engine | Proven today | Important open boundary |
| --- | --- | --- |
| Compatibility / QEMU | Safe plans; explicit image creation and inspection; supervised launch/stop; NAT/forwards; snapshot and diagnostics paths | Privileged macOS vmnet modes, full guest/GUI coverage, public packaging |
| Apple VZ | Signed Linux Arm runner; raw disk plus direct-kernel boot; display/control socket and framebuffer export | Wider boot media/disk formats, full desktop integration, live CPU/RAM reapply, release packaging |
| Windows HVF | Installed Windows desktop; 4 vCPUs; writable NVMe/UEFI vars; RAMFB/input; virtio network/audio/agent; clean shutdown/restart; Venus/VirGL experimental 3D and real PPSSPP Vulkan evidence; live Windows TPM TIS command path; local PPI/TPM2 log; signed bundled swtpm; Keychain recovery/clone/reset lifecycle | Windows PPI action and measured-boot receipt; clean-second-Mac migration and BitLocker recovery proof; fresh WDK package and production signature; current real-title receipt; public signing/notarization |

## Windows HVF evidence boundary

The installed-Windows path has live evidence for:

- clean system-off, NVMe flush/writeback, and post-exit reopen;
- in-process Windows restart with BridgeVM device, guest RAM, vCPU, and Apple
  in-kernel GIC reset;
- resident BVAGENT command execution and bounded chunked output;
- guest-requested shutdown and first-boot disk-growth actions;
- a bound experimental Windows ARM64 display stack, including host-driven
  resolution changes applied by the guest;
- host-visible Vulkan rendering and PPSSPP 1.20.4 running with its native
  Vulkan backend (dated 2026-07-23; reproducible again since the 2026-08-01
  BAR2 fix, though without a process-attributed frame rate);
- deferred scanout/readback instrumentation that identifies synchronous
  GPU-to-CPU readback as a major remaining display cost.
- a live Windows vTPM run with 1,032 TIS commands, including PCR, capability,
  session, key-creation, and NV-public operations, with no backend or malformed
  packet failures.

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
| `SEC-TPM-FRONTEND` | `LIVE_PROVEN` | Bounded device-model work | Windows ACPI/TIS enumeration, live command flow, and a live PPI **clear** action (request → reboot → F12 → `TPM2_CC_Clear` → post-clear desktop → clean off) are all dated-receipt proven |
| `SEC-TPM-LIFECYCLE` | `LIVE_PROVEN` | Hard security/lifecycle work | Authenticated encrypted export on Mac Studio [A] and packaged-app same-ID restore/desktop boot on MacBook Pro M5 [B] are dated-receipt proven with 239 `PCR_Read`, zero malformed/backend failures, and clean PSCI shutdown; BitLocker-only C3/C6 are blocked by Windows Home edition |
| `SEC-SB-MEASURED` | `LIVE_PROVEN` | Hard cross-layer work | Windows reports Secure Boot enabled and TPM 2.0 ready; a 62,382-byte measured-boot log, 431 `PCR_Read`, 82 `PCR_Extend`, and zero malformed/backend failures are dated-receipt proven |
| `GPU-WDK-SIGN` | `SUBMISSION_READY` | Production signature externally gated | ARM64 package/catalog/test signature/readiness, exact bind, Vulkan draw and host trace are proven; production EV/Partner Center signature remains external |
| `GPU-LIVE-RECEIPT` | `LIVE_PROVEN` | Live-machine gated | Packaged-app 120.41 bind → native ARM64 PPSSPP 1.20.4 Vulkan UI for >10 min → 300-second `fb-rate.py` result (13.54 FPS average) → clean shutdown. Unreproducible from 2026-07-23 until the 2026-08-01 BAR2 fix; instance creation now succeeds 5/5. A process-attributed title frame rate is still unproven |
| `DIST-MACOS` | `EXTERNAL` | Externally gated | Developer ID, hardened runtime, notarization, clean-machine install and launch |

The TPM register model and ACPI plumbing are comparatively straightforward.
Correct identity, migration, BitLocker recovery, signing, and reproducible live
evidence are not “easy last steps”; they are the release-quality work.

Current `SEC-TPM-FRONTEND` evidence is E4 for both the Windows TIS command path
and a live PPI **clear** action: five TIS localities, command FIFO, the 1 KiB PPI
mailbox, PPI 1.3/reset-mitigation `_DSM`, fixed MMIO
dispatch, optional ACPI `TPM0/MSFT0101`, and the revision-4 TPM2 table with a
loader-relocated 64 KiB `etc/tpm/log` area are unit proven. BridgeVM now also
publishes QEMU's exact packed 6-byte `etc/tpm/config` discovery record only
when a concrete TPM backend is present. The pinned ArmVirtQemu EDK2 firmware's
`Tcg2PhysicalPresenceLibQemu` can therefore discover the PPI page, initialize
its supported-operation policy, and process pending requests during the boot
manager phase. Presence and exact record bytes, plus absence when TPM is
disabled, are regression tested. The installed-boot
launcher now owns a fail-closed swtpm process/socket lifecycle and preserves its
per-VM state directory. The app product configuration supplies a per-VM
256-bit `WhenUnlockedThisDeviceOnly` Keychain key through a one-shot inherited
FD; swtpm encrypts state with AES-256-CBC plus encrypt-then-MAC, and an existing
state directory with a missing key is never assigned a silent replacement key.
On 2026-07-22, a 120-second cloned Windows run reached the desktop and completed
1,032 TPM commands: 975 successful responses, 186 `StartAuthSession`, three
`CreatePrimary`, 40 `NV_ReadPublic`, 146 `PCR_Read`, and 81 `PCR_Extend`, with
zero backend failures and zero malformed commands or responses. A later
20-second diagnostic run with the patched firmware completed 483 TPM commands,
20 PPI reads, and 276 PPI writes with no backend failure, malformed traffic, or
firmware exception. On 2026-07-22 a fresh same-process PPI clear then completed
end to end on a disposable clone: `Clear-Tpm -UsePPI` set `RestartPending=True`,
an in-process reboot reached the firmware caution prompt, a live F12 delivered
between the two resets approved it, the firmware executed one
`TPM2_ClearControl` and one `TPM2_CC_Clear` (both `TPM_RC_SUCCESS`, `clear=1` in
the summary, 266 PPI writes), Windows returned to the desktop with
`RestartPending=False`, and the guest powered off cleanly with zero backend or
malformed failures. Closing it required two fixes kept payload-free: the vTPM is
now power-cycled (swtpm `CMD_INIT`) on guest reset so volatile platform
authorization does not persist, and the pinned firmware now processes the PPI
request before locking the platform hierarchy (rebuilt reproducibly to SHA-256
`b1dc201b…`). The replay defect remains regression-tested and fixed and F12
approval remains supported. The dated PPI-clear receipt and its `--ppi-action`
verifier mode are indexed in the
[PPI clear evidence](docs/windows-arm/evidence/vtpm-windows-ppi-clear-20260722.md);
the prior command-path receipt is in the
[command-path evidence](docs/windows-arm/evidence/vtpm-windows-command-path-20260722.md).
The repository smoke proves exact 32-byte FD delivery, socket/process cleanup,
and persistent state without putting the key in argv or a disk keyfile.
The packaged app now carries pinned swtpm 0.10.1/libtpms 0.10.2 plus the entire
rewritten non-system dylib closure, signatures, SHA-256 inventory, and license
notices. The app exposes authenticated recovery-package export/restore,
archive-before-reset, and APFS clone with a fresh TPM identity; copied state,
old state, orphan keys, and prior run evidence are retained with lifecycle
receipts instead of silently discarded. On 2026-07-23 the final package again
passed a real 32-byte binary key-FD, Unix data/control socket, encrypted-state,
and process-cleanup smoke. The same day, Mac Studio [A] exported an authenticated
recovery package and a quasi-clean MacBook Pro M5 [B] used only the transferred
package/app/state/vars/disk to restore the same stable identity. Windows reached
the desktop without a recovery screen, completed 239 `PCR_Read` and 82
`PCR_Extend` operations with zero malformed/backend failures, then accepted an
agent shutdown, reached PSCI system-off, wrote back NVMe, and cleaned up. See the
[second-Mac migration receipt](docs/windows-arm/evidence/second-mac-migration-20260723.md).
BitLocker enable/clone contrast remains explicitly `BLOCKED_BY_EDITION` because
the guest is Windows 11 Home (`EditionID=Core`) and no Pro key was available.

`SEC-SB-MEASURED` also has deterministic local evidence. The default firmware
is a reproducible EDK2 build pinned to commit
`b03a21a63e3bd001f52c527e5a57feddb53a690b` with Secure Boot and TPM2 enabled;
its 3 MiB code volume is pinned by SHA-256. Fresh-install finalization validates
Microsoft's ARM64 `secureboot_objects` v1.6.5 payloads and source provenance,
then writes `dbx`, `db`, `KEK`, and `PK` in that order. Exact state is
idempotent; partial, duplicate, or conflicting state fails without mutation.
The packaged path includes the policy, build receipt, and license notices.
The 2026-07-23 live guest receipt now raises this to E4: Windows returned
`Confirm-SecureBootUEFI=True`, `SetupMode=0`, `SecureBoot=1`, and a ready TPM
2.0; the resident agent retrieved the complete 62,382-byte measured-boot log,
and the host observed 431 `PCR_Read` plus 82 `PCR_Extend` operations with zero
malformed commands/responses or backend failures. See the
[Secure Boot measured-boot receipt](docs/windows-arm/evidence/sb-guest-proof-20260723.md).

The graphics release boundary was live on 2026-07-23: the ARM64 Venus package is
submission-ready apart from production signing, and a packaged-app 120.41 run
kept native ARM64 PPSSPP 1.20.4 rendered on Venus for more than ten minutes.
The same boot produced 11,293 scanout readbacks, a passing P3 trace, and a
300-second framebuffer result of 13.54 FPS average before clean PSCI shutdown.
See the [GPU package receipt](docs/windows-arm/evidence/viogpu3d-venus-release-candidate-20260723.md)
and [GPU live receipt](docs/windows-arm/evidence/gpu-live-receipt-20260723.md).

That receipt went unreproduced for over a week. The cause was found and fixed on
2026-08-01: the cached virtio-gpu BAR2 base was refreshed only from cpu0, so a
secondary-CPU BAR reprogram left `hv_vm_map` installing the Venus ring at a
stale address and every guest ring access was swallowed by the RAZ/WI shm
handler. Vulkan instance creation now succeeds in five runs out of five.

Two limits remain. No real title has yet produced a process-attributed frame
rate, so the A2/A3 FPS gates are still open. And an intermittent boot stall
still costs boots on this image; it is unrelated to the graphics defect and is
not fixed.

The stall has since been narrowed to a swallowed virtual-timer fire around
host-initiated vCPU cancellation. Recovering the timer on every canceled exit
moved the diagnostic gate to 11/12, but one boot still stalled in
`KeIpiGenericCall` with an expired, unmasked timer, so the mechanism is a
strongly measured cause rather than a completed root-cause proof and **A1
remains open**. See
[the boot stall investigation](docs/windows-arm/evidence/a1-boot-stall-20260802.md).

Two security contract defects are also open release blockers, both found by
static review of the probe's SMCCC dispatch and confirmed in current code. The
guest-visible TRNG is a deterministic function of the vCPU exit counter while
being advertised as a true random number generator, and the PSCI `CPU_ON` and
`AFFINITY_INFO` implementations do not follow the specified state table. Both
must be fixed before any release wording changes.

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
