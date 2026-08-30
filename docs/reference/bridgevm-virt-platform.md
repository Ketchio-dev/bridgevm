# BridgeVM `virt` platform register map

Document status: **Reference**
Last reviewed: **2026-08-30**

This document describes BridgeVM's guest-visible platform. It is written from
BridgeVM's own source of truth in
[`machine.rs`](../../crates/bridgevm-hvf/src/machine.rs) and the public
interfaces named below. It is not generated from another virtual machine
monitor and is not an implementation template.

The normative source for an address or interrupt number is `machine.rs`. This
page is a review aid; deterministic tests reject overlaps and verify that the
FDT and ACPI builders use the declared constants.

## Public interface sources

| Surface | Governing interface |
|---|---|
| CPU power and reset | Arm PSCI 1.1 and SMCCC |
| Interrupt controller | Arm GIC Architecture Specification, GICv3 |
| UART | Arm PrimeCell UART PL011 TRM, DDI 0183G |
| Real-time clock | Arm PrimeCell RTC PL031 TRM |
| Device tree | Devicetree Specification, FDT version 17 |
| Paravirtual I/O | OASIS Virtual I/O Device (VIRTIO) Specification |
| PCI configuration | PCI Express ECAM and PCI firmware interfaces |
| TPM | Trusted Computing Group PC Client Platform TPM Profile |
| Firmware tables | UEFI and ACPI specifications |

The project also implements compatibility-only firmware interfaces whose
identifiers are fixed by the firmware ABI. Keeping such an identifier is an
interoperability requirement; it does not make the implementation a copy of
the program that first published the interface.

## Memory map

| Region | Base | Size | Purpose |
|---|---:|---:|---|
| firmware code | `0x0000_0000` | `0x0400_0000` | read-only pflash bank |
| firmware variables | `0x0400_0000` | `0x0400_0000` | writable pflash bank |
| GIC distributor | `0x0800_0000` | `0x0001_0000` | GICv3 distributor |
| MSI frame | `0x0808_0000` | `0x0000_1000` | message-signalled SPI window |
| GIC redistributors | `0x080a_0000` | `0x00f6_0000` | per-vCPU redistributors |
| PL011 UART | `0x0900_0000` | `0x0000_1000` | serial console/debug port |
| PL031 RTC | `0x0901_0000` | `0x0000_1000` | wall-clock device |
| firmware configuration | `0x0902_0000` | `0x0000_0018` | firmware data channel |
| PL061 GPIO | `0x0903_0000` | `0x0000_1000` | power-button GPIO |
| virtio-mmio transports | `0x0a00_0000` | `0x0000_4000` | 32 slots, `0x200` bytes each |
| TPM TIS localities | `0x0c00_0000` | `0x0000_5000` | TPM 2.0 FIFO/TIS |
| TPM PPI page | `0x0c00_5000` | `0x0000_0400` | physical-presence handoff |
| PCI 32-bit MMIO | `0x1000_0000` | `0x2eff_0000` | below-4-GiB BAR space |
| PCI port I/O | `0x3eff_0000` | `0x0001_0000` | root-bridge I/O window |
| system RAM | `0x4000_0000` | configured | guest memory |
| PCI ECAM | `0x40_1000_0000` | `0x1000_0000` | buses 0 through 255 |
| PCI 64-bit MMIO | `0x80_0000_0000` | `0x80_0000_0000` | high BAR space |

## Interrupt map

GIC SPIs use absolute INTID `SPI + 32`. PL011 uses SPI 1, PL031 uses SPI 2,
PCI INTx uses SPIs 3 through 6, PL061 uses SPI 7, and virtio-mmio slot `i`
uses SPI `16 + i`. The Arm generic timer PPIs are 13 (secure physical), 14
(non-secure physical), 11 (virtual), and 10 (hypervisor). The PMU uses PPI 7.

## Change control

Any guest-visible change must update `machine.rs`, its deterministic tests, and
[`qemu-virt-deviations.json`](../machine-contract/qemu-virt-deviations.json)
when it changes the compatibility contract. A static document never replaces a
live gate receipt for Windows behavior.
