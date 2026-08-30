# BridgeVM Virtual ARM PC v1

Document status: **Experimental contract**

Last reviewed: **2026-08-30**

This is the versioned guest contract for a future BridgeVM-owned virtual Arm
PC. It is defined from public Arm system, UEFI, ACPI, SMBIOS, PSCI/SMCCC,
GICv3, PCIe, NVMe, xHCI, TCG and OASIS virtio interfaces. It does not imitate a
physical Mac or a proprietary OEM Arm board.

The current product does not boot this board. The proven Windows path keeps its
existing board contract until this platform passes independent UEFI, install,
storage, network, display/input, reset, security and reliability gates.

## Stable identity

- Board ID: `com.ketchio.bridgevm.virtual-arm-pc`
- ABI version: `1`
- SMBIOS manufacturer: `Ketchio`
- SMBIOS product: `BridgeVM Virtual ARM PC`
- OS discovery: UEFI configuration tables, ACPI and SMBIOS
- Firmware handoff: `BridgeVM boot-info v1`

A VM bundle records both the board ID and ABI version. A runtime must refuse an
unknown version instead of silently attaching an existing disk to a different
guest-visible machine.

## v1 address map

The normative values live in
[`bridgevm_pc.rs`](../../crates/bridgevm-hvf/src/bridgevm_pc.rs).

| Region | Base | Size |
|---|---:|---:|
| firmware code | `0x0000_0000` | `0x0400_0000` |
| firmware variables | `0x0400_0000` | `0x0400_0000` |
| GICv3 distributor | `0x2000_0000` | `0x0001_0000` |
| GICv3 redistributors | `0x2100_0000` | `0x0200_0000` |
| MSI frame | `0x2300_0000` | `0x0001_0000` |
| PL011 debug UART | `0x2400_0000` | `0x0000_1000` |
| PL031 RTC | `0x2401_0000` | `0x0000_1000` |
| TPM 2.0 TIS | `0x2500_0000` | `0x0000_5000` |
| BridgeVM boot-info | `0x2600_0000` | `0x0001_0000` |
| PCIe ECAM | `0x4000_0000` | `0x1000_0000` |
| PCIe 32-bit MMIO | `0x5000_0000` | `0xb000_0000` |
| system RAM | `0x1_0000_0000` | configured |
| PCIe 64-bit MMIO | `0x20_0000_0000` | `0x20_0000_0000` |

The board has no legacy paravirtual MMIO array and no compatibility firmware
configuration device. System storage, xHCI input, installer media, network,
display, guest-agent transport and audio are PCIe functions at `00:01.0`
through `00:07.0`, respectively.
The runtime must query Hypervisor.framework's GIC region sizes, alignments and
supported SPI range and fail closed if this map cannot be configured.

The first 2026-08-30 live placement probe correctly failed because the draft
reserved only 4 KiB for the MSI frame while this Mac's Hypervisor.framework
reported a 64 KiB size and alignment. The unreleased v1 draft was corrected to
64 KiB, then a repeated probe created the GIC at the v1 distributor,
redistributor and MSI addresses. This is a live single-run host-placement
receipt, not evidence that firmware or Windows boots this board.

## Firmware and Windows boundary

The firmware must publish ACPI and SMBIOS pointers through the standard UEFI
configuration table and provide the UEFI services and protocols Windows uses,
including block I/O, GOP, variables, time and reset. The ACPI platform is
hardware-reduced and describes GICv3, PCIe, TPM and the control-method power
button. PSCI owns vCPU startup, shutdown and reset coordination.

`BridgeVM boot-info v1` is a private firmware-internal handoff only. Windows
must not depend on it; after firmware processing, Windows sees standard UEFI,
ACPI, SMBIOS, PCIe and device interfaces.

## Implementation gates

| Gate | State |
|---|---|
| Versioned identity, address map and overlap tests | implemented, static only |
| Minimal memory layout, vars flash, UART, RTC and PCIe ECAM runtime | implemented, host-unit only |
| Host GIC geometry validation and bounded placement probe | live single-run placement passed; integration open |
| HVF memory mapping and device interrupt routing | open |
| Independently built and audited UEFI firmware | open |
| UEFI ACPI/SMBIOS/GOP/block-I/O handoff | open |
| Windows installer and installed-disk boot | open |
| Storage, input, network, graphics and reset live gates | open |
| TPM, Secure Boot and recovery lifecycle | open |
| Fixed-sample reliability and performance gates | open |

Static tests establish only that the contract is internally coherent. They do
not establish that firmware or Windows can boot it.

## Public specifications

- [Arm SystemReady standards](https://developer.arm.com/Architectures/Arm%20SystemReady)
- [UEFI 2.11](https://uefi.org/specs/UEFI/2.11/)
- [ACPI specifications](https://uefi.org/specifications)
- [Windows UEFI requirements for SoC platforms](https://learn.microsoft.com/windows-hardware/drivers/bringup/uefi-requirements-that-apply-to-all-windows-platforms)
- [OASIS Virtual I/O Device specification](https://docs.oasis-open.org/virtio/virtio/v1.3/virtio-v1.3.html)
