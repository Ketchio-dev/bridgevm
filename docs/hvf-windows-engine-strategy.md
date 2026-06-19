# BridgeVM HVF Windows engine — strategy & sequenced plan

_Last updated: 2026-06-19._

## Context

The BridgeVM HVF engine is the Phase 0 R&D track aimed at booting Windows 11 ARM
on Apple Silicon **without QEMU**, on Hypervisor.framework directly. A June 2026
external audit was checked against the live tree and is accurate: the HVF crate is
a single 34.7k-line probe harness centred on **FDT**, a **userspace GIC skeleton**,
and **virtio-mmio** — none of which is the right spine for a Windows target. The
companion document
[`docs/hvf-windows-platform-contract-gap.md`](hvf-windows-platform-contract-gap.md)
quantifies why: the smokes already load QEMU's ArmVirtQemu firmware, but the
platform under it reproduces only the RAM base of the QEMU `virt` contract, and is
missing `fw_cfg` and PCIe ECAM entirely.

## The decision: Path A — converge on the QEMU `virt` contract

Two coherent ways forward; mixing them (QEMU firmware on a non-QEMU platform) is
the current broken middle state.

### Path A — "become QEMU `virt`" (CHOSEN)

Implement `fw_cfg`, PCIe ECAM, GICv3 (Apple `hv_gic` or modelled), and the QEMU
`virt` memory map + DTB. Then:

- Stock `edk2-aarch64-code.fd` boots **unmodified**.
- The firmware/QEMU-style table flow generates **ACPI for free** — little or no
  hand-written ACPI in Rust.
- The **same Windows 11 ARM media that installs under QEMU installs under
  BridgeVM**, because the guest sees a bit-identical platform.
- The existing QEMU Compatibility engine becomes a true differential oracle: any
  divergence is a bug in our device models, diffable against a known-good stack.

Cost: we implement QEMU's paravirtual contract (the `fw_cfg` protocol, the DTB
ArmVirtQemu consumes, ECAM, GICv3/ITS). This is bounded, well-specified work with a
reference implementation to diff against.

### Path B — own platform + own firmware (REJECTED for now)

Define a clean-room `bridgevirt-v0`, hand-write ACPI (RSDP/XSDT/FADT/MADT/GTDT/
MCFG/SPCR/DBG2/DSDT) in Rust, and maintain a custom EDK2/ArmVirtPkg platform port
that targets those tables. Maximum control, but it means owning an EDK2 fork and a
from-scratch ACPI generator, and every table address/IRQ/checksum is a place
Windows can die silently after the boot manager. This is the audit's implicit path
and a multi-year effort. Revisit only if Path A hits a hard wall (e.g. a QEMU
contract detail that cannot be reproduced under HVF).

> **Refinement of the audit:** under Path A, FDT is **not** deleted. ArmVirtQemu
> consumes a DTB; the current DTB is just wrong (no `fw_cfg`/PCIe nodes, wrong
> addresses). The work is to make the DTB a faithful QEMU-`virt` DTB, while ACPI is
> produced *above* the platform and delivered through `fw_cfg` — never as a device
> tree to the guest OS.

## Strategic honesty: is the custom Windows VMM even the highest-value track?

Stated plainly so it is not lost: **QEMU + HVF already boots Windows 11 ARM on the
CPU axis today.** Parallels' real edge is **GPU/WDDM/guest-tools integration**,
which is orthogonal to whether the CPU runs on QEMU or a bespoke VMM. The custom
HVF engine is therefore the most expensive track with the least *user-visible*
payoff. Legitimate reasons to still pursue it: distribution/licensing (owning the
stack instead of shipping GPL QEMU), startup overhead, and a polished product
identity. The current `PLAN.md` staging — "restricted QEMU/HVF for Windows first,
long-term custom HVF VMM" — is correct and should be preserved. Path A is the
*cheapest* version of the custom track precisely because it reuses the QEMU
contract; do not let the from-scratch framing (Path B) rush this track ahead of the
display/guest-tools work that moves the Parallels-class needle sooner.

## Sequenced plan (ordering, not a calendar)

Realistic effort for a solo/small team is **days-to-weeks per step**, not a day
each. The ordering is what matters. The single best de-risk is step 6 (Linux
ACPI-only boot): Linux gives you `dmesg`, Windows gives you a sad face.

| # | Step | State | Notes |
| --- | --- | --- | --- |
| 0 | Decide Path A; record contract gap | **done** | this doc + the gap doc + checked-in reference DTS |
| 1 | `fw_cfg` device model (selector/data + DMA) | **done (modelled + unit-tested)** | `crates/bridgevm-hvf/src/fwcfg.rs`; HVF MMIO wiring still to do |
| 2 | `virt` machine model + QEMU-shaped DTB generator | **done (modelled + `dtc`-verified)** | `src/machine.rs` (single source of truth + no-overlap validator) and `src/dtb.rs` (`build_virt_fdt`, decompiles `dtc`-clean against the contract). Wiring the map into the live run loop is step 3. |
| 3 | Assemble the `virt` platform + `fw_cfg` behind one MMIO-exit entry point; feed `etc/acpi/tables`/`etc/acpi/rsdp`/SMBIOS/boot order | **done (assembled + unit-tested)** | `src/platform_virt.rs` (`VirtPlatform`): owns the map, the populated `fw_cfg`, the DTB and the guest-memory layout; `on_mmio()` is the single call the live run loop makes. Only the `hv_vcpu_run` call itself (step 6) needs an entitled host. |
| 4 | GICv3: spike Apple `hv_gic_create` (macOS 15+, create before vCPUs); else model GICv3+ITS at QEMU bases | after 2 | replaces the userspace skeleton on the product path |
| 5 | PCIe ECAM (`pci-host-ecam-generic`) + config space + MSI/MSI-X | after 4 | prerequisite for NVMe/virtio-pci |
| 6 | **Linux ACPI-only boot** through the stock firmware | after 3–5 | the oracle: confirm ACPI/GIC/timer/PCIe before touching Windows |
| 7 | NVMe controller (identify + admin/IO queues) on PCIe | after 5 | Windows Setup has an inbox NVMe driver; no inbox virtio |
| 8 | Windows Boot Manager / Setup first attempt; capture deterministic failure trace | after 6–7 | success = a reproducible "where it died", diffed against QEMU |
| 9 | GOP framebuffer + keyboard | after 8 | Setup UI + "press any key" |
| 10 | vTPM 2.0, Secure Boot, virtio-net/guest agent | later | Windows 11 compliance + usability |

## What is done in this change

- **Decision recorded** (Path A) with rationale and the rejected alternative.
- **Contract gap quantified** against the real dumped QEMU `virt` DTB, with a
  checked-in reference at `docs/reference/qemu-virt-aarch64-gicv3.dts`.
- **Path A bricks landed (steps 1–2):**
  - `crates/bridgevm-hvf/src/fwcfg.rs` — spec-correct `fw_cfg` model (14 tests).
  - `crates/bridgevm-hvf/src/machine.rs` — the `virt` machine model (memory map +
    IRQ map + GICv3 sizing), single source of truth, with a no-overlap validator
    that fails on exactly the collision class the gap doc found (9 tests).
  - `crates/bridgevm-hvf/src/dtb.rs` — an FDT/DTB serializer + `build_virt_fdt`,
    which emits a QEMU-`virt`-shaped device tree from `machine.rs`. Verified by
    decompiling the output with `dtc` (zero warnings) and confirming every device
    address against the contract (5 tests + `examples/emit_virt_dtb.rs`).

  - `crates/bridgevm-hvf/src/platform_virt.rs` — `VirtPlatform`, which assembles
    the map + populated `fw_cfg` + DTB + guest-memory layout behind one
    `on_mmio()` entry point (6 tests, incl. a fw_cfg DMA transfer routed through
    guest RAM via the platform).

  Full crate suite green at **129 passing**, zero warnings. New platform code
  lives in its own modules — the de-monolithing pattern the audit asked for,
  applied to surviving code rather than a big-bang refactor of the probe harness.

### Live integration point — validated on real Hypervisor.framework

**This is no longer hypothetical.** Hypervisor.framework is usable directly on an
Apple Silicon dev host: ad-hoc code-signing grants `com.apple.security.hypervisor`
(`codesign --sign - --entitlements hv.entitlements --force <bin>`), no paid
Developer ID or separate machine required. The first end-to-end proof passes today
— see `examples/hvf_fw_cfg_live.rs` and
`tests/integration/hvf-fw-cfg-mmio-live-opt-in-smoke.sh`:

```
MMIO read @ 0x09020000 size 1 -> 0x51 into x0   (real guest data abort)
guest X0 = 0x51 ('Q')                            (fw_cfg signature, via VirtPlatform)
LIVE PROOF: real guest MMIO -> VirtPlatform::on_mmio -> fw_cfg -> guest saw 'Q'
```

i.e. a real guest vCPU's MMIO read was trapped by HVF, decoded, and routed through
`VirtPlatform::on_mmio` into the `fw_cfg` device, and the guest observed the
correct byte. The whole Path A platform is driven by exactly this one call from the
`hv_vcpu_run` data-abort (MMIO) exit handler:

```rust
// In the run loop, on an HVF_EXIT_REASON data abort:
let op = if is_write { MmioOp::Write { size, value } } else { MmioOp::Read { size } };
match platform.on_mmio(fault_ipa, op, &mut guest_ram) {
    MmioOutcome::ReadValue(v) => set_guest_register(dst_reg, v),
    MmioOutcome::WriteAck => {}
    MmioOutcome::KnownUnimplemented(name) => trace!("MMIO to unmodelled {name} @ {fault_ipa:#x}"),
    MmioOutcome::Unmapped => trace!("MMIO to unmapped {fault_ipa:#x}"),
}
```

`guest_ram` is a [`fwcfg::GuestMemoryMut`] view over the HVF-mapped RAM; in the
live example it is `FlatGuestRam`. The remaining bring-up — growing this loop to
map pflash + RAM per `VirtPlatform::memory_layout()`, load `edk2-aarch64-code.fd`,
place the DTB at `dtb_load`, and model GICv3/timer/PCIe/NVMe — is pure engineering,
and **every step is now live-verifiable on this host** the same way the fw_cfg
proof is. The step-6 Linux ACPI-only boot is the milestone, not a hardware gate.

### Honest status — stock EDK2 firmware boots to DXE on this platform

The biggest validation: **the unmodified ArmVirtQemu firmware
(`edk2-aarch64-code.fd`) boots on the Path A platform.** Loaded at flash `0x0` with
the generated DTB at the DRAM base, it runs through PEI into DXE and prints its
banner *through the modelled PL011 UART*:

```
UEFI firmware (version edk2-stable202408-prebuilt.qemu.org ...)
InitializeVirtioFdtDxe: Failed to install VirtIO transport @ 0xA000000 ...  (×32)
```

The `InitializeVirtioFdtDxe` lines prove the firmware **parsed the generated device
tree** and walked the virtio-mmio nodes at exactly the machine-map addresses. So
`fw_cfg`, the DTB, and the UART all work against real firmware. See
`examples/hvf_edk2_boot_probe.rs` + `hvf-edk2-boot-live-opt-in-smoke.sh`.

It stopped with a system-register trap (ESR EC `0x18`) after the GIC accesses — so
GICv3 was the next blocker, exactly as the audit's P0 predicted.

### GICv3 via Apple `hv_gic` — firmware now reaches DXE proper

`hv_gic_create` is available on this host (macOS 15+), so the GIC is provided
**in-kernel by Apple** instead of hand-modelled (`examples/hvf_gic_boot_probe.rs` +
`hvf-gic-boot-live-opt-in-smoke.sh`). With the redistributor relocated to fit
Apple's 32 MiB region (`machine::GIC_REDIST` @ `0x0c000000`) and minimal PSCI +
empty-ECAM + dropping the >IPA-size 64-bit PCIe window, the firmware now:

- passes GIC CPU-interface init — the EC `0x18` system-register trap is **gone**
  (Apple serves the distributor + CPU interface);
- handles **PSCI** (HVC) and an **empty PCIe bus** (ECAM returns all-ones);
- runs **into DXE proper** (DxeCore), past the previous frontier.

**The GIC is now fully served by Apple** — distributor, redistributor, and CPU
interface, with zero GIC MMIO trapping to userspace. The missing key was
**`MPIDR_EL1`**: Apple associates each vCPU's redistributor frame from its MPIDR
affinity, so it must be set (`hv_vcpu_set_sys_reg(HV_SYS_REG_MPIDR_EL1, 0x80000000)`)
before the redistributor MMIO is served. With it,
`hv_gic_get_redistributor_base(vcpu0)` returns `0x80a0000` and `gic-redist` drops
out of the unmodelled list entirely. (The redistributor stays at QEMU virt's
`0x080a0000`; Apple's `redistributor_region_size` is a *maximum*, not a required
reservation — confirmed against QEMU's own hvf backend.)

**The firmware boots deep into DXE driver dispatch.** Fixes landed this far, each
moving the frontier later: PSCI (HVC), empty PCIe ECAM (all-ones config reads),
dropping then re-adding the right PCIe windows, fw_cfg big-endian selector/DMA, the
`/flash@0` node (VirtNorFlashDxe — doubled execution to ~16.8k exits), and creating
the VM with the **max IPA size** (40-bit; the ECAM is at 256 GiB).

**RngDxe — FIXED. Root cause: an HVC PC double-advance in the run loop.** Found by
running **3 parallel worktree agents** (a QEMU+HVF oracle differential, a guest-RAM
disassembly with live instrumentation, and a DTB-node bisection). The data-abort
exit handler advances PC by +4 to step over the emulated faulting instruction — but
HVF reports the **HVC** exit PC *already past* the `hvc` (data aborts report it *at*
the faulting instruction). The extra +4 skipped `ArmCallHvc`'s `ldr x9, [sp], #0x10`
(the SMCCC args-pointer reload), so RngDxe's first SMCCC call stored its result
through a **stale x9** (`0xFF_FFFFFFFF`) and faulted at `0x100000000F`. It was never
a TRNG/ACPI/flash/DTB-node problem — all three were ruled out in parallel; the QEMU
oracle (which boots the same firmware to the UEFI shell) confirmed the firmware/HVF
side is fine. Fix: on an HVC exit, set `PC = last_pc` (no +4); also answer
`SMCCC_VERSION`. RngDxe now starts, two other optional drivers unload gracefully
(`Image start failed: Not Found`, exactly as under QEMU), and the firmware runs on
**through late DXE**.

**Current frontier — interrupt delivery.** The firmware now hard-spins in EDK2's
`MmioRead32` (`0x5fcf13b0`, `ldr w0,[x0]; dsb; ret`) with the exit count frozen and
`vtimer = 0` — i.e. it is polling a register served in-kernel by Apple `hv_gic`,
waiting for an **interrupt that never fires**. The next subsystem is **architected
timer / interrupt delivery via `hv_gic`** (handle `HV_EXIT_REASON_VTIMER_ACTIVATED`
and route the timer PPI through the in-kernel GIC, plus `hv_gic_set_spi` for SPI
lines). Then NVMe, Linux ACPI / Windows. No external host, paid entitlement, or
separate machine is in the way; the whole loop, including the QEMU oracle, is
live-debuggable here.
