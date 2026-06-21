# BridgeVM HVF Windows engine — strategy & sequenced plan

_Last updated: 2026-06-21._

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
| 4 | GICv3: spike Apple `hv_gic_create` (macOS 15+, create before vCPUs); else model GICv3+ITS at QEMU bases | **done (Apple `hv_gic`, live)** | distributor/redistributor, timer delivery and the MSI frame are served by Hypervisor.framework |
| 5 | PCIe ECAM (`pci-host-ecam-generic`) + config space + MSI/MSI-X | **partial (wired + Linux validated)** | ECAM host bridge, NVMe endpoint config space, BAR0 MMIO routing, writable MSI-X table/PBA, Apple GICM/GICv2m-style MSI-frame metadata and `hv_gic_send_msi` delivery are wired; Linux drives the NVMe queue through the PCI endpoint under ACPI |
| 6 | **Linux ACPI-only boot** through the stock firmware | **partial (Ubuntu root userspace starts)** | QEMU-style `-kernel`/`-initrd` fw_cfg blobs boot Ubuntu's arm64 kernel through EFI, ACPI, SMBIOS/DMI, GIC, timer, PL011 console binding, ACPI0007 CPU device enumeration, PCI root enumeration, QEMU-like PCI `_OSC`, basic PPTT CPU topology, PMU IRQ metadata, root ext4 mount, `/boot` and `/boot/efi` mounts, `sysinit.target`, and `basic.target`. The ECAM PnP reservation warning is present in the QEMU+HVF oracle too, so the active BridgeVM-only gaps are now post-boot services, missing devices such as network/display/input, and Windows validation rather than early ACPI metadata. |
| 7 | NVMe controller (identify + admin/IO queues) on PCIe | **partial (Linux root boot validated)** | the controller is reachable through PCIe BAR0; raw host-file media is wired into the live boot probe with read-only sparse overlays or write-through mode; PRP1/PRP2/PRP-list transfers, including PRP2 list-pointer offsets, are handled; Linux no longer reports the previous large-read `SC_INVALID_FIELD` / ext4 journal-abort failure |
| 8 | Windows Boot Manager / Setup first attempt; capture deterministic failure trace | **partial (Windows Setup GUI reached)** | With `BRIDGEVM_RAMFB=1`, stock ArmVirtQemu firmware, the PCI `virtio-blk-pci` installer ISO at `00:03.0`, a serial-marker space injected at `BdsDxe: starting Boot0001`, and a separate GPT raw NVMe target disk, BridgeVM reaches Windows 11 Setup's `Select language settings` ramfb GUI. The prior `Install driver to show hardware` screen came from booting without a usable writable target disk, not from a missing firmware/loader ISO path. |
| 9 | GOP framebuffer + keyboard/input | partial (ramfb proof only) | QEMU ramfb fw_cfg is wired enough for snapshots and Windows Setup UI evidence. PL011 marker injection is sufficient for the boot prompt in the probe, but production input still needs a guest-visible keyboard path. |
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

  Full crate suite green at **230 passing**, zero warnings. New platform code
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

### Current frontier — Linux root boot to Windows installer validation

Firmware boot is no longer the frontier, and neither is the old late-DXE
`0x5fcf13b0` polling hang. The live probe now has three useful OS-level proofs:

- stock ArmVirtQemu firmware reaches the UEFI shell under Apple `hv_gic`;
- QEMU direct Linux boot blobs (`BRIDGEVM_LINUX_KERNEL`/`INITRD`/`CMDLINE`) reach
  Debian's arm64 installer kernel, ACPI interpreter enablement, GIC/timer init,
  initramfs unpack, and `Run /init as init process`.
- a real Ubuntu 24.04 ARM raw root disk attached through the modelled NVMe PCIe
  endpoint reaches `Welcome to Ubuntu 24.04.4 LTS!`, mounts the root filesystem,
  mounts `/boot` and `/boot/efi`, reaches `sysinit.target` and `basic.target`,
  and no longer logs the previous `nvme0n1: I/O Cmd ... sc 0x2` /
  `EXT4-fs error` cascade.
- QEMU-style SMBIOS blobs in `fw_cfg` are installed by firmware and consumed by
  Linux, which now logs `SMBIOS 3.0.0 present` and a BridgeVM `DMI:` line instead
  of `DMI not present or invalid`.

#### 2026-06-21 ramfb Windows Setup proof

The ramfb path is now repeatable with boot-start input injection. Use the same
ad-hoc HVF signing recipe as the live opt-in tests, then run:

```sh
BRIDGEVM_RAMFB=1 \
BRIDGEVM_INSTALLER_ISO=/Users/user/Downloads/Win11_25H2_English_Arm64_v2.iso \
BRIDGEVM_UART_RX_ON_SERIAL_MARKER=' ' \
BRIDGEVM_UART_RX_SERIAL_MARKER='BdsDxe: starting Boot0001' \
BRIDGEVM_RAM_MIB=4096 \
BRIDGEVM_BOOT_PROBE_WATCHDOG_MS=90000 \
BRIDGEVM_RAMFB_DUMP_DIR=.omo/ulw-loop/evidence/ramfb-g001-marker-trigger \
target/debug/examples/hvf_gic_boot_probe
```

Live evidence:
`.omo/ulw-loop/evidence/G001-C001-ramfb-marker-trigger-live-hvf.txt` shows
`hv_vm_create(ipa=40) = 0x0`, `ramfb fw_cfg: enabled`, and
`UART RX injection serial-marker: fired=true bytes=1`. The same run records
PCI boot-media `requests=234 reads=234 bytes_read=646239744 status=0x0` and
dumps a non-empty 800x600 ramfb snapshot whose PNG shows Windows 11 Setup at
`Install driver to show hardware`.

That screen was useful because it proved the loader and GUI were alive, but the
next focused run narrowed the meaning of the failure. Booting the same PCI
`virtio-blk-pci` installer ISO while also attaching a separate GPT raw NVMe disk
as the writable target moved the ramfb snapshot to Windows 11 Setup's
`Select language settings` page:

```sh
BRIDGEVM_RAMFB=1 \
BRIDGEVM_INSTALLER_ISO=/Users/user/Downloads/Win11_25H2_English_Arm64_v2.iso \
BRIDGEVM_NVME_DISK=/tmp/bridgevm-g002-winsetup-source.raw \
BRIDGEVM_TRACE_NVME=1 \
BRIDGEVM_RECENT_NVME_COMMANDS=192 \
BRIDGEVM_UART_RX_ON_SERIAL_MARKER=' ' \
BRIDGEVM_UART_RX_SERIAL_MARKER='BdsDxe: starting Boot0001' \
BRIDGEVM_RAM_MIB=4096 \
BRIDGEVM_BOOT_PROBE_WATCHDOG_MS=120000 \
BRIDGEVM_RAMFB_DUMP_DIR=.omo/ulw-loop/evidence/ramfb-g002-disk-shaped-nvme-source \
target/debug/examples/hvf_gic_boot_probe
```

Live evidence:
`.omo/ulw-loop/evidence/G002-C002-disk-shaped-nvme-source-live-hvf.txt` exits
cleanly after serving the same 234 PCI boot-media requests and reading
`646239744` bytes from the ISO. Its ramfb PNG at
`.omo/ulw-loop/evidence/ramfb-g002-disk-shaped-nvme-source/ramfb-800x600-13c7a0000-0ccc66f0651d4a64.png`
shows `Select language settings`, not `Install driver to show hardware`. Treat
that older driver page as a no-target-disk experiment unless it reappears with a
separate writable NVMe target attached. The serial tail still prints
`ConvertPages: failed to find range ...`; keep it in traces, but the Setup GUI and
successful ISO/target-disk path show it is not the current stop by itself.

The first ACPI device-parity gaps are now closed: BridgeVM's generated DSDT names
QEMU-like `ACPI0007` CPU devices, the `ARMH0011` PL011 console, `PNP0A08` `PCI0`
root bridge, `PNP0C02` ECAM reservation and `PNP0C0C` power button, and
`PCI0._OSC` follows QEMU's host-bridge policy. The MADT now follows QEMU's GICv3
shape as well: revision 4, GICD before GICC entries, and PMU PPI 7 advertised as
performance-interrupt GSIV `0x17`. The Linux oracle now logs `ACPI: CPU0 has been
hot-added`, `legacy console [ttyAMA0] enabled`, `ACPI: PCI Root Bridge [PCI0]`,
`_OSC: OS now controls [PCIeHotplug SHPCHotplug PME AER PCIeCapability]`, `ECAM
area ... reserved by PNP0C02:00`, `PnP ACPI: found 1 devices`, and starts
installer userspace instead of printing `Warning: unable to open an initial
console.` A QEMU-like PPTT is installed, so Linux no longer reports `No PPTT table
found` or `cacheinfo: Unable to detect cache hierarchy for CPU 0`; QEMU-style
SMBIOS removes the earlier `DMI not present or invalid` warning; and the latest
live HVF run no longer logs the previous `topology_sysfs_init`, `cpuinfo`, or
`No ACPI PMU IRQ for CPU0` diagnostics. The ECAM PnP reservation warning also
appears under the QEMU+HVF oracle with the same firmware and Linux command line,
so it is not currently a BridgeVM-only diff. QEMU-style `_OSC` still correctly
does not grant LTR.

The remaining OS-boot contract work is now narrower:

- keep the QEMU-style ACPI delivery wired through `fw_cfg` entries
  `etc/acpi/rsdp`, `etc/acpi/tables` and `etc/table-loader`;
- keep tightening the installer ISO path. The live QEMU/HVF oracle shows the
  Windows ISO as `PciRoot(0x0)/Pci(0x2,0x0)/CDROM(0x0)` and reaches `Press any
  key to boot from CD or DVD...` only when ACPI is enabled. With `acpi=off`,
  Windows Boot Manager fails early with `BlInitializeLibrary failed 0xc0000225`.
  Attaching the raw ISO as the existing BridgeVM NVMe namespace is not a shortcut:
  stock firmware creates a BridgeVM NVMe boot option but fails it with `Not
  Found` and falls through to the shell. The current BridgeVM live probe now
  exposes the installer ISO as read-only PCI `virtio-blk-pci` at `00:03.0` by
  default, with `BRIDGEVM_INSTALLER_ISO_TRANSPORT=mmio` preserving the older
  legacy virtio-mmio slot-31 path as a fallback. This is fixed read-only block
  media: it deliberately does not claim QEMU's true CD-ROM/removable-media or
  xHCI/USB-storage semantics. The PCI boot-media parity work is tracked by
  `.omo/ulw-loop/evidence/task-5-bridgevm-hvf-pci-boot-media-parity-device-shape.txt`
  and `.omo/ulw-loop/evidence/task-5-bridgevm-hvf-pci-boot-media-parity-example-check.txt`;
  the follow-up live proof should land in
  `.omo/ulw-loop/evidence/bridgevm-hvf-pci-boot-media-parity-live-hvf.txt`. The
  latest live probe exposes PMUVer (`ID_AA64DFR0_EL1[11:8] = 1`) so cdboot no
  longer traps on PMU register access, and `BRIDGEVM_UART_RX_ON_CD_PROMPT=' '`
  injects serial input only after the CD prompt is printed. With the previous
  legacy-mmio path the loader prints `Loading files...`, reads roughly 300 MiB in
  a 30 s run and roughly 646 MiB in a 120 s run with zero virtio I/O errors, and
  reaches Windows high virtual-address code (`pc=0xfffff801...`). Recent 120 s
  traces end in Windows
  high-VA code (one data-abort snapshot, one watchdog snapshot with
  `ESR=0x56001004`/SVC state). The live probe now reads the EL1 translation
  controls and walks the guest's 4 KiB stage-1 tables through the reusable
  `src/stage1.rs` helper, so the latest watchdog snapshot resolves
  `pc=0xfffff80145081cdc` through `TTBR1_EL1` to `ipa=0x100481cdc`, inside
  `ntkrnlmp.pdb` at RVA `0x481cdc`. Watchdog dumps also walk a bounded saved-FP
  frame chain (`FRAMECHAIN`, default 12 frames, capped at 64 through
  `BRIDGEVM_FRAME_CHAIN_LIMIT`) and resolve saved LR values through the same
  stage-1 helper, giving kernel RVAs such as `0x519f6c`, `0x2c3d88`,
  `0x518434`, and `0x50e358` in `ntkrnlmp.pdb`. The stop dump now prints the
  full x0-x28 GPR set, decodes SVC/HVC ESR immediates, and emits a small
  branch-aware AArch64 instruction-word summary for translated PC/LR/ELR code
  windows. The latest Windows stop still carries SVC immediate `0x1004` in EL1
  exception state, but the watchdog PC itself is in `ntkrnlmp.pdb` RVA
  `0x481cdc`, immediately after a decoded `dsb sy; isb sy; wfi` idle sequence
  (`/tmp/bridgevm-arm64-insn-60s.out`). That makes the next QEMU diff an
  idle-wake/timer/interrupt/device-shape question rather than an HVF SVC-exit
  question. The PCIe MMIO tail repeatedly reads NVMe `CSTS`/`CC` and rings
  `SQ0TDBL`, so the next diff is Windows NVMe/PCIe command flow and device-shape
  parity rather than the old
  late-DXE poll, the cdboot stub writer, basic ISO reachability, or interrupt
  delivery. The live probe now prints the recent PCIe MMIO tail with decoded
  NVMe register names (`BRIDGEVM_RECENT_PCIE_MMIO`), e.g. `nvme.SQ0TDBL`,
  `nvme.CQ0HDBL`, `nvme.SQ1TDBL`, `nvme.CSTS`, `nvme.CC` and `nvme.ASQ`,
  which makes queue-doorbell churn visible without re-decoding BAR offsets by
  hand. The same tail now emits a ranked register summary, and the latest
  Windows run shows the expected admin/I/O queue doorbells plus optional
  no-CMB probes (`nvme.CMBLOC`/`nvme.CMBSZ`, both reading zero), with no
  unmodelled MMIO. It also keeps a bounded NVMe command/completion ring
  (`BRIDGEVM_RECENT_NVME_COMMANDS`) so the next long run can identify the
  repeated SQE, its decoded LBA/count or admin selector, PRPs/CDWs, completion
  status, interrupt route, expected pending AERs versus other pending commands,
  and repeated command signatures directly. The first
  Windows-observed NVMe admin-command gaps are now
  closed: Asynchronous Event Request commands are accepted and left pending,
  standard `Get Features` probes return boring defaults, `Identify` CNS `0x06`
  succeeds for the NVM command set, QEMU's command-effects log page `0x05` is
  modelled, firmware-slot log page `0x03` completes, and Security Send/Receive
  opcodes are advertised with QEMU's default no-SPDM behavior. Windows currently
  issues two zero-length `SECURITY_RECV` probes and probes optional/vendor
  surfaces (`Get Features` FID `0xd0`/`0x7f` and log pages `0xc0`/`0xc1`);
  BridgeVM now matches QEMU's `invalid-field | DNR` status for those unsupported
  query paths. The 120 s post-DNR live run
  (`/tmp/bridgevm-nvme-dnr-120s.out`) still reaches `Loading files...`, reads
  `645730816` bytes from the ISO with zero virtio I/O errors, and stops at the
  same Windows high-VA/SVC frontier (`pc=0xfffff80335681cdc`,
  `ESR=0x56001004`) with no unmodelled MMIO or `invalid-opcode` completions. The
  volatile write cache surface now matches QEMU's observed shape: Identify
  Controller advertises VWC `0x7`, `Get Features` FID `0x06` reports the current
  cache enabled, and NVM Flush (`0x00`) completes for both namespace and
  broadcast-NSID requests. Later 60 s register-summary runs reach the same
  Windows high-VA/SVC frontier class, with stops resolving into `ntkrnlmp.pdb`
  RVAs such as `0x481cdc` and `0x4e0d78`, only the expected four pending
  Asynchronous Event Requests, and no other pending NVMe commands. The current
  evidence no longer points at missing NVMe completions.
- keep tightening ACPI parity that matters for Windows/Linux device paths. DBG2
  now matches QEMU's PL011 debug-port shape; Apple `hv_gic` still lacks
  guest-visible LPIs/ITS, so current MSI routing is advertised as a MADT Generic
  MSI Frame instead of MADT ITS + IORT;
- lift the raw-image NVMe overlay/writeback path from the live boot probe into
  the eventual engine-facing VM configuration and keep extending it toward
  production persistence semantics;
- lift the pflash variable snapshot/writeback hooks from the live boot probe into
  the eventual engine-facing VM configuration (`src/media.rs` now holds the
  reusable host-file policy) so boot order and NVRAM state survive repeated runs
  outside ad-hoc probes;
- decide and implement the next guest-visible install-media path for Windows
  Setup. The leading candidates are true QEMU-parity CD-ROM/removable media
  semantics, xHCI USB mass storage once the controller has enough operational
  model, or an inbox-visible NVMe-backed target/install-media arrangement. Keep
  the diff empirical: QEMU+HVF reaches this phase with `-cdrom`; BridgeVM's fixed
  virtio block media is enough for firmware and loader reads, but not enough for
  Setup's hardware/media discovery screen.

No external host, paid entitlement, or separate machine is in the way; the whole
loop, including the QEMU oracle, is live-debuggable here.
