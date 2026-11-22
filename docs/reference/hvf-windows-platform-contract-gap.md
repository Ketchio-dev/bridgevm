# HVF Windows firmware-compatibility boundary

_Last updated: 2026-08-30._

## Status

This is a historical design note. It explains why BridgeVM's current HVF
platform contains a narrow QEMU-compatible firmware boundary; it is not an
implementation specification and does not make another VMM's source material a
design authority.

The normative BridgeVM platform sources are:

- [`crates/bridgevm-hvf/src/machine.rs`](../../crates/bridgevm-hvf/src/machine.rs)
- [`crates/bridgevm-hvf/src/platform_virt/`](../../crates/bridgevm-hvf/src/platform_virt)
- [`bridgevm-virt-platform.md`](bridgevm-virt-platform.md)
- [`qemu-virt-deviations.json`](../machine-contract/qemu-virt-deviations.json)

The current release path still boots a pinned ArmVirtQemu EDK2 firmware image.
That binary expects the published `fw_cfg` wire ABI, including the
`qemu,fw-cfg-mmio` device-tree identifier and fixed protocol signatures. Those
names are retained as interoperability identifiers. BridgeVM implements the
surrounding virtual platform and device behavior in Rust.

## Original failure

The first HVF probe loaded the pinned firmware over a different experimental
memory map. Its flash, interrupt-controller, UART, RTC, PCIe and legacy virtio
windows did not describe one coherent machine. Firmware failed before it could
publish the ACPI and SMBIOS data needed by Windows, and early experiments tried
to repair control flow after each fault.

That conclusion is retained as a failed design: patching vectors or register
state cannot substitute for a consistent guest-visible platform contract.

The most important original collisions were:

| Surface | Early probe | Firmware-compatible platform | Consequence |
| --- | ---: | ---: | --- |
| code flash | `0x0800_0000` | `0x0000_0000` | overlapped the interrupt-controller region expected by the firmware |
| GIC distributor | `0x1001_0000` | `0x0800_0000` | interrupt discovery and routing disagreed |
| PL011 UART | experimental device window | `0x0900_0000` | console discovery disagreed |
| PL031 RTC | experimental device window | `0x0901_0000` | timer/RTC discovery disagreed |
| PCIe ECAM | absent | `0x40_1000_0000` | firmware could not enumerate PCI endpoints |
| `fw_cfg` | absent | `0x0902_0000` | firmware could not receive generated ACPI, SMBIOS or boot metadata |
| RAM | `0x4000_0000` | `0x4000_0000` | already consistent |

Historical QEMU/HVF control runs helped distinguish a firmware-contract failure
from an HVF execution failure. They are retained as experiment evidence only;
BridgeVM behavior is specified by its own machine contract, public hardware
standards and live BridgeVM gate receipts.

## Current boundary

The compatibility layer is intentionally small and explicit:

- `fwcfg.rs` implements the selector/data and DMA wire protocol required by the
  pinned firmware.
- `tpm_ppi.rs` publishes the packed PPI discovery record consumed by that
  firmware.
- the DTB advertises the legacy `qemu,fw-cfg-mmio` identifier because changing
  it would prevent the current firmware from binding.
- protocol literals such as `QEMU` and `QEMU CFG` remain byte-exact where the
  wire ABI requires them.

These identifiers do not authorize copying another VMM's implementation.
Behavior outside this boundary is defined from public specifications such as
Arm GIC, PL011, PL031, PCIe, NVMe, xHCI, ACPI, SMBIOS, TCG and virtio, plus
BridgeVM's declared contract and tests.

## Independent-platform direction

An independent BridgeVM-owned board (the BridgeVM Virtual ARM PC, with its own
firmware package and boot-info handoff) was prototyped through UEFI BDS exit in
2026-08/09 and removed in 2026-11 because the shipped app never used it; its
receipts remain in `docs/windows-arm/evidence/` (the `bridgevm-pc-` files). The existing
contract and its documented deviations are the shipping truth. A future
independent board would need to pass the same real-hardware boot, installation,
input, graphics, storage, security and performance gates as the current
platform before replacing it.

## Evidence discipline

- Compatibility-engine runs may mention QEMU directly in commands, logs and
  receipts.
- Historical comparisons remain failures or controls; they are never rewritten
  into release proof.
- A source-level similarity claim is not inferred from protocol compatibility.
- Guest-visible changes require an entry in
  [`qemu-virt-deviations.json`](../machine-contract/qemu-virt-deviations.json)
  and live validation at the stated sample count.
