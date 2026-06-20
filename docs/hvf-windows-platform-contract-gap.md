# HVF Windows path: platform-contract gap vs. QEMU `virt`

_Last updated: 2026-06-20._

> **Status note:** this document records the original contract gap that motivated
> Path A. The active Path A source of truth is now
> [`crates/bridgevm-hvf/src/machine.rs`](../crates/bridgevm-hvf/src/machine.rs) plus
> [`crates/bridgevm-hvf/src/platform_virt.rs`](../crates/bridgevm-hvf/src/platform_virt.rs).
> That implementation now boots stock ArmVirtQemu firmware to the UEFI shell and
> QEMU direct Linux boot blobs through Debian installer userspace startup. The legacy
> `src/lib.rs` probe map below is retained as historical context, not as the
> desired machine model.

## Why this document exists

The live HVF firmware smokes (`tests/integration/windows-arm-hvf-real-edk2-*`)
load **QEMU's** ArmVirtQemu firmware build:

```
FIRMWARE = /opt/homebrew/Cellar/qemu/11.0.1/share/qemu/edk2-aarch64-code.fd
VARS     = /opt/homebrew/Cellar/qemu/11.0.1/share/qemu/edk2-arm-vars.fd
```

That firmware does **not** boot on an arbitrary platform. ArmVirtQemu discovers
its world from the device tree QEMU hands it and from the `fw_cfg` device, and it
installs the **guest ACPI tables that QEMU generates and exposes through
`fw_cfg`** (`etc/acpi/tables`, `etc/acpi/rsdp`, `etc/table-loader`). It expects a
GICv3 (with an ITS for MSI), a `pci-host-ecam-generic` root complex, PL011/PL031,
and flash banks — all at the QEMU `virt` addresses, described by a QEMU-shaped DTB.

Originally `bridgevm-hvf` presented almost none of that and loaded QEMU's firmware
onto a **non-QEMU platform**. That mismatch — not a firmware bug — was the root
cause of the `try-recommended-vbar` / `low-vector-repair` /
`restore-before-eret` / `diagnostic` vector-patching seen in the early firmware
run-loop smokes: the firmware faulted early because the hardware underneath it was
not the hardware it was built for, and the run loop tried to patch around the
faults instead of supplying the contract.

> **Windows 11 ARM consumes ACPI, not a device tree.** A DTB handed to the
> *firmware* is fine and expected (that is how ArmVirtQemu works); the DTB just has
> to describe the QEMU-shaped platform, including the `fw_cfg` and PCIe nodes, so
> the firmware can emit ACPI for the guest. The current DTB does neither.

## The authoritative contract (regenerate it yourself)

Everything below was dumped from the **exact** QEMU the smokes use, so it is the
real contract, not from memory:

```sh
qemu-system-aarch64 -machine virt,gic-version=3,dumpdtb=virt.dtb \
  -cpu cortex-a72 -m 6144 -smp 4
dtc -I dtb -O dts virt.dtb -o virt.dts
```

A decompiled reference copy is checked in at
[`docs/reference/qemu-virt-aarch64-gicv3.dts`](reference/qemu-virt-aarch64-gicv3.dts).
Re-dump it whenever the bundled QEMU version changes — the addresses are stable
across recent QEMU releases but the ECAM/highmem layout can shift with options.

### QEMU `virt` device map (GICv3, QEMU 11.0.1)

| Node (DT) | Base | Size | `compatible` |
| --- | --- | --- | --- |
| `flash@0` (pflash code+vars) | `0x0000_0000` | 2 × `0x0400_0000` | `cfi-flash` |
| `intc@8000000` GICD | `0x0800_0000` | `0x0001_0000` | `arm,gic-v3` |
| `intc@8000000` GICR | `0x080A_0000` | `0x00F6_0000` | `arm,gic-v3` |
| `its@8080000` (MSI) | `0x0808_0000` | `0x0002_0000` | `arm,gic-v3-its` |
| `pl011@9000000` UART | `0x0900_0000` | `0x0000_1000` | `arm,pl011` |
| `pl031@9010000` RTC | `0x0901_0000` | `0x0000_1000` | `arm,pl031` |
| `fw-cfg@9020000` | `0x0902_0000` | `0x0000_0018` | `qemu,fw-cfg-mmio` |
| `virtio_mmio@a000000…` (32) | `0x0A00_0000` | 32 × `0x200` | `virtio,mmio` |
| `pcie@10000000` ECAM | `0x40_1000_0000` | `0x1000_0000` | `pci-host-ecam-generic` |
| PCIe 32-bit MMIO window | `0x1000_0000` | `0x2EFF_0000` | (ranges) |
| PCIe 64-bit MMIO window | `0x80_0000_0000` | `0x80_0000_0000` | (ranges) |
| `memory@40000000` RAM | `0x4000_0000` | guest size | — |

GIC: `gic-version=3`, `#interrupt-cells = <3>`, ITS is `msi-controller`.

### Legacy `bridgevm-hvf` probe map (`crates/bridgevm-hvf/src/lib.rs`)

| Constant | Base |
| --- | --- |
| `WINDOWS_ARM_UEFI_LOW_CODE_ALIAS_IPA` | `0x0000_0000` |
| `WINDOWS_ARM_UEFI_LOW_VARS_ALIAS_IPA` | `0x0400_0000` |
| `WINDOWS_ARM_UEFI_CODE_IPA` (pflash) | `0x0800_0000` |
| `WINDOWS_ARM_UEFI_VARS_IPA` (pflash) | `0x0C00_0000` |
| `WINDOWS_ARM_DEVICE_MMIO_IPA` (window) | `0x1000_0000` (256 MiB) |
| └ virtio installer ISO MMIO | `0x1000_2000` |
| └ virtio target disk MMIO | `0x1000_3000` |
| └ GIC distributor MMIO | `0x1001_0000` |
| └ GIC redistributor MMIO | `0x1002_0000` |
| `WINDOWS_ARM_GUEST_RAM_IPA` | `0x4000_0000` |
| platform DTB | `0x4001_0000` |

## The original gap, side by side

| Capability | QEMU `virt` (the firmware's contract) | `bridgevm-hvf` today | Verdict |
| --- | --- | --- | --- |
| **`fw_cfg`** | `0x0902_0000`, `qemu,fw-cfg-mmio` | **absent** (0 references) | **MISSING — keystone.** No path for the guest ACPI tables, SMBIOS, boot order, kernel/initrd. ArmVirtQemu has nothing to install → ACPI-dependent Windows cannot boot. |
| **PCIe ECAM** | `pci-host-ecam-generic` @ `0x40_1000_0000` | **absent** (0 references) | **MISSING.** No NVMe, no virtio-pci, no MSI-targeted devices. |
| **GIC ITS (MSI)** | `its@8080000`, `msi-controller` | **absent** | **MISSING.** No MSI/MSI-X for PCIe devices. |
| GIC distributor | `0x0800_0000` (in-kernel `hv_gic` or modelled) | userspace skeleton @ `0x1001_0000` | MISMATCH — wrong base, and a hand-rolled model (see [strategy](hvf-windows-engine-strategy.md)). |
| GIC redistributor | `0x080A_0000` | userspace skeleton @ `0x1002_0000` | MISMATCH. |
| pflash (code) | `0x0000_0000` | `0x0800_0000` | MISMATCH — bridgevm's code pflash sits exactly where QEMU puts the **GIC distributor**. |
| PCIe 32-bit MMIO window | `0x1000_0000` | reused as the device MMIO window | COLLISION. |
| PL011 / PL031 | `0x0900_0000` / `0x0901_0000` | inside the `0x1000_0000` window | MISMATCH (address). |
| virtio transport | virtio-**mmio** @ `0x0A00_0000`, 0x200 stride | virtio-mmio @ `0x1000_2000`/`0x1000_3000` | MISMATCH (base, stride, count); and virtio-mmio is the wrong transport for an inbox-driver Windows install (see strategy). |
| RAM base | `0x4000_0000` | `0x4000_0000` | ✅ **MATCH — the only one.** |

**Original bottom line:** of QEMU `virt`'s entire device map, the legacy HVF probe
map reproduced only the RAM base. The two genuinely missing subsystems —
`fw_cfg` and PCIe ECAM (+ ITS) — were precisely the ones that gated ACPI and
storage, i.e. the ones that gate Windows booting at all.

## What this implies (see the strategy doc for the decision)

There are two coherent ways to stop loading QEMU firmware onto a non-QEMU platform.
The chosen direction is **Path A — converge on the QEMU `virt` contract** so the
stock `edk2-aarch64-code.fd` boots unmodified and the guest ACPI/PCIe/Windows-media
behaviour matches the QEMU stack that already installs Windows 11 ARM. Rationale,
the rejected alternative (Path B, own platform + own EDK2 + hand-written ACPI), and
the sequenced plan live in
[`docs/hvf-windows-engine-strategy.md`](hvf-windows-engine-strategy.md).

Path A now has `fw_cfg`, a QEMU-shaped DTB, Apple `hv_gic`, PL011, PL031, empty
virtio-mmio slots, PCIe ECAM host-bridge config space, a first NVMe endpoint at
`00:01.0` with BAR0 routing and raw host-file media hooks, and a minimal P30
pflash vars model wired behind `VirtPlatform::on_mmio()` with live-probe
snapshot/writeback hooks. The stock ArmVirtQemu firmware boots to the UEFI shell.
ACPI blobs are now delivered through QEMU-style `etc/acpi/rsdp`,
`etc/acpi/tables` and `etc/table-loader` fw_cfg files, and SMBIOS blobs are
delivered through `etc/smbios/smbios-anchor` and `etc/smbios/smbios-tables`.
QEMU-style Linux `-kernel`/`-initrd`/`-append` fw_cfg blobs now boot Debian's
arm64 installer kernel through EFI, ACPI, SMBIOS/DMI, GIC/timer init,
`ARMH0011` PL011 console binding, `PCI0` root bridge enumeration, QEMU-like PCI
`_OSC`, ACPI0007 CPU device enumeration, basic PPTT CPU topology, PMU IRQ
metadata, ECAM reservation through `PNP0C02`, initramfs unpack, root ext4 mount,
`/boot` and `/boot/efi` mounts, `sysinit.target`, and `basic.target`. The latest
live HVF run no longer logs the previous
`topology_sysfs_init`, `cpuinfo`, `cacheinfo`, `No PPTT table found`, `No ACPI PMU
IRQ`, or invalid-DMI diagnostics. The ECAM PnP reservation warning is also
present in the QEMU+HVF oracle, so it is no longer treated as a BridgeVM-only
platform gap. The current Apple `hv_gic` path deliberately advertises the MSI
surface as a GICv2m-compatible Generic MSI Frame (Apple's GICM registers) rather
than MADT ITS + IORT: the in-kernel GIC does not expose guest-visible LPIs/ITS,
while Linux falls back to the MSI-frame driver when the GIC distributor lacks LPI
support. The Windows ISO oracle now narrows the installer-media gap: QEMU/HVF with
ACPI enabled and `-cdrom` exposes the ISO as `.../CDROM(0x0)` and reaches
`Press any key to boot from CD or DVD...`; the same ISO attached as BridgeVM's
raw NVMe namespace fails the firmware boot option with `Not Found`. BridgeVM's
new virtio-mmio block ISO prototype on slot 31 is discovered by firmware and
services reads successfully, but Windows loader execution now stops after
`ConvertPages` failures, so the active gap has moved from basic ISO reachability
to memory-map/device-shape parity with the QEMU oracle.
The remaining gap is above firmware: lift NVMe overlay/writeback and
pflash persistence into the engine-facing VM configuration, keep tightening
Windows-relevant ACPI details such as DBG2 as needed, add installer usability
devices such as GOP framebuffer, keyboard/input and networking, and then run
Windows installer validation.
