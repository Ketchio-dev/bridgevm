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
| 1 | `fw_cfg` device model (selector/data + DMA) | **done (modelled + live-wired)** | `crates/bridgevm-hvf/src/fwcfg.rs`; exercised through `VirtPlatform::on_mmio()` in the live HVF boot probe |
| 2 | `virt` machine model + QEMU-shaped DTB generator | **done (modelled + `dtc`-verified)** | `src/machine.rs` (single source of truth + no-overlap validator) and `src/dtb.rs` (`build_virt_fdt`, decompiles `dtc`-clean against the contract). Wiring the map into the live run loop is step 3. |
| 3 | Assemble the `virt` platform + `fw_cfg` behind one MMIO-exit entry point; feed `etc/acpi/tables`/`etc/acpi/rsdp`/SMBIOS/boot order | **done (assembled + live-wired)** | `src/platform_virt.rs` (`VirtPlatform`): owns the map, populated `fw_cfg`, DTB, ACPI table-loader blobs, guest-memory layout and MMIO dispatch; `on_mmio()` is the single call the live run loop makes |
| 4 | GICv3: spike Apple `hv_gic_create` (macOS 15+, create before vCPUs); else model GICv3+ITS at QEMU bases | **done (Apple `hv_gic`, live)** | distributor/redistributor + timer delivery are served by Hypervisor.framework; ITS/MSI remains separate work |
| 5 | PCIe ECAM (`pci-host-ecam-generic`) + config space + MSI/MSI-X | **partial** | ECAM host bridge, NVMe endpoint config space and BAR0 MMIO routing are wired; MSI/MSI-X delivery is still pending |
| 6 | **Linux ACPI-only boot** through the stock firmware | after 3–5 | the oracle: confirm ACPI/GIC/timer/PCIe before touching Windows |
| 7 | NVMe controller (identify + admin/IO queues) on PCIe | **partial** | minimal controller and admin/IO queues exist and are reachable through PCIe BAR0; boot-media backing, interrupt/MSI behavior and OS boot validation remain |
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
live examples it is either `FlatGuestRam` or a direct view over the mapped HVF RAM.
That loop now maps pflash/RAM, loads `edk2-aarch64-code.fd`, places the generated
DTB at `dtb_load`, serves Apple `hv_gic`, and routes Path A MMIO through
`VirtPlatform`. The remaining work is no longer "can firmware execute"; it is the
guest-OS contract above firmware: PCIe endpoint/BAR routing, NVMe storage, and
then Linux ACPI-only / Windows install attempts.

### Honest status — stock ArmVirtQemu reaches UEFI Shell

The biggest validation: **the unmodified ArmVirtQemu firmware
(`edk2-aarch64-code.fd`) reaches the UEFI shell on the Path A platform.** The live
proof is `examples/hvf_gic_boot_probe.rs` +
`tests/integration/hvf-gic-boot-live-opt-in-smoke.sh`, which ad-hoc signs the
example with the Hypervisor entitlement, boots with Apple `hv_gic`, and now asserts
the shell banner:

```
BdsDxe: starting Boot0001 "EFI Internal Shell"
UEFI Interactive Shell v2.2
Shell>
```

This is true for the QEMU prebuilt release firmware and for the local DEBUG EDK2
build with symbol logs. The DEBUG build is still valuable because it emits
`add-symbol-file ...` lines that `examples/edk2_symbolize_log.rs` can resolve
against the EDK2 `.debug` files, but it is no longer required to get past DXE.

### What moved the frontier

The major late-DXE stall was not a timer, interrupt, ACPI, or ISR-delivery bug.
The writable pflash variable bank had been mapped as plain writable RAM. EDK2's
`VirtNorFlashDxe` talks to that bank through Intel P30/StrataFlash command and
status sequences, so command writes into raw RAM corrupted the backing bytes and
left the firmware polling forever for a write-ready status bit. `src/pflash.rs`
now models the small subset EDK2 needs (array reads, status reads, ID/CFI probes,
word/buffered program and erase), and the live probe leaves the vars bank unmapped
so those accesses trap into `VirtPlatform::on_mmio()`.

Other fixed bring-up blockers remain important traps for future work:

- Apple `hv_gic` serves distributor, redistributor and CPU-interface state in
  kernel; set `MPIDR_EL1 = 0x80000000` before redistributor service.
- HVC exits report PC already past the `hvc`; data-abort exits still need PC + 4.
- `fw_cfg` selector and DMA registers are big-endian.
- The VM must use the max IPA size (40-bit here) because the PCIe ECAM sits at
  256 GiB.
- The DTB needs the QEMU `virt` `/flash@0` node and must omit the 64-bit PCIe MMIO
  window until the guest IPA aperture can represent it.

### Current frontier — OS boot contract

Firmware boot is no longer the frontier. The next milestone is an ACPI-only Linux
boot through the stock firmware, because Linux gives a useful `dmesg` oracle before
Windows. To get there:

- keep the QEMU-style ACPI delivery wired through `fw_cfg` entries
  `etc/acpi/rsdp`, `etc/acpi/tables` and `etc/table-loader`;
- turn the minimal in-memory NVMe BAR0 path into a real boot-media path with
  host/disk backing and interrupt/MSI behavior;
- persist pflash variable writes back to a vars image so boot order and NVRAM state
  survive repeated runs;
- then boot Linux with ACPI, diff against QEMU+HVF, and only then try Windows Setup.

No external host, paid entitlement, or separate machine is in the way; the whole
loop, including the QEMU oracle, is live-debuggable here.
