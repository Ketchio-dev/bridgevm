# BridgeVM Virtual ARM PC v1

Document status: **Experimental contract**

Last reviewed: **2026-08-31**

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

A second bounded probe at exact BridgeVM code head
`5db0948751d41f2f8f200e01431f6c7ea5ee355e` mapped executable guest RAM at
`0x1_0000_0000`, created the same host GIC, configured its system-register CPU
interface and virtual-timer PPI from a minimal EL1 guest, and observed that
guest's IRQ handler write `flag=1`. It produced no
`HV_EXIT_REASON_VTIMER_ACTIVATED` exit, demonstrating in-kernel PPI delivery at
the independent board addresses. This remains one live interrupt-path run, not
a firmware, Windows, or device-SPI result. See the
[exact-head receipt](../windows-arm/evidence/bridgevm-pc-gic-irq-live-20260830.md).

A third bounded probe at exact BridgeVM code head
`d7a9d191b8acd37939a94e5ef2f14cceb83d8d13` mapped the complete 64 KiB
boot-info image read-only at `0x2600_0000`. A minimal EL1 guest validated the
`BVMBOOT1` magic, ABI v1, 112-byte header checksum and RSDP signature, followed
the RSDP pointer to `0x2600_2000`, and validated the XSDT signature before
returning result `1`. This proves live mapping and guest visibility only; no
firmware phase executed. See the
[exact-head receipt](../windows-arm/evidence/bridgevm-pc-boot-info-live-20260830.md).

A fourth bounded probe at exact BridgeVM code head
`5ed86f6bfa9bfb615bd2308178002710aa34f958` placed a reproducibly built
64 MiB development flash image at GPA zero and entered its BridgeVM-owned
AArch64 reset code there. The primary path validated the `BVMBOOT1` handoff at
`0x2600_0000`, stored result `1` in system RAM and returned through HVC. This is
one live reset-entry run; the image still contains no SEC C environment, PI
HOB list, DXE core or UEFI services. See the
[exact-head receipt](../windows-arm/evidence/bridgevm-pc-reset-vector-live-20260830.md).

A fifth bounded probe at exact BridgeVM code head
`2d097a066d6a9868df5a99b8c67ab504e3d9c046` extended that reset path. It
set a stack at `0x1_0002_0000`, crossed the AArch64 C ABI, and ran a
freestanding BridgeVM SEC validator that checked the full boot-info header,
checksum, reserved fields, table ranges, RAM geometry and CPU count before
returning result `1`. This is one live reset-to-SEC run; it still constructs no
PI HOB list and enters no DXE core. See the
[exact-head receipt](../windows-arm/evidence/bridgevm-pc-sec-live-20260830.md).

A sixth bounded probe at exact BridgeVM code head
`10a0aae10b22e2aa88780fcac17088d7acdd83e8` added PI HOB construction.
After validating boot-info v1, SEC wrote a five-entry, 176-byte list at
`0x1_0000_4000`: PHIT, system-memory resource, stack allocation, CPU and end
HOBs. An independent host parser validated every field after the HVC exit.
This is one live reset-to-HOB run; it still contains no firmware volume and
enters no DXE core. See the
[exact-head receipt](../windows-arm/evidence/bridgevm-pc-hob-live-20260830.md).

A seventh bounded probe at exact BridgeVM code head
`183571b5a0d60d44d84e9eb7b62b6d5d69959b0e` embedded a pinned generic DXE
Core and a BridgeVM-owned marker driver in a development firmware volume. The
IPL appended FV and module-allocation HOBs, entered the fixed-rebased core, and
the core created an EFI system table and dispatched the marker. An independent
host parser required seven exact HOBs, marker stage `8` and the standard system
table signature. This is one live DXE-dispatch run, not complete UEFI or
Windows boot evidence. See the
[exact-head receipt](../windows-arm/evidence/bridgevm-pc-dxe-dispatch-live-20260830.md).

An eighth bounded probe at exact BridgeVM code head
`290f6b9c9c72c5659b53e1ae7ae10c8407cda720` inserted the BridgeVM-owned
platform-table driver before the marker. The driver validated the complete
boot-info ACPI and SMBIOS payloads and installed the standard ACPI 2.0 and
SMBIOS 3 configuration-table GUIDs. After the marker returned, an independent
host parser walked the EFI system table and required seven bounded entries plus
the exact RSDP and SMBIOS anchor pointers. This is one live platform-table
publication run, not complete UEFI or Windows boot evidence. See the
[exact-head receipt](../windows-arm/evidence/bridgevm-pc-platform-tables-live-20260830.md).

A ninth bounded probe at exact BridgeVM code head
`0890d838e1dbc20f0ab393646023d2d8014a9b91` inserted the pinned generic
`RuntimeDxe` before both BridgeVM drivers. The marker's dependency expression
required the standard Runtime Architectural Protocol, located that protocol,
checked the runtime-services table and called `CalculateCrc32`. An independent
host parser matched the returned service pointers to their table slots and
revalidated the ACPI and SMBIOS entries. This is one live RuntimeDxe run, not
proof of variables, virtual-address transition, BDS, or Windows boot. See the
[exact-head receipt](../windows-arm/evidence/bridgevm-pc-runtime-dxe-live-20260830.md).

A tenth fixed-sample live probe at exact BridgeVM code head
`cf8ad54f0f99f87e574d6af152ea8b64fe3d6d6f` added generic
`VariableRuntimeDxe`, real AArch64 cache maintenance for DXE image loading and
a reserved three-page identity map. In each of 20 independent processes, the
first HVF VM wrote a non-volatile UEFI variable, was destroyed, and a second VM
with fresh RAM restored the exact payload from the preserved host-memory vars
backing. All 20 lanes passed while retaining the runtime, ACPI and SMBIOS
checks. This does not prove process-, reboot- or power-loss persistence, BDS or
Windows boot. See the
[exact-head receipt](../windows-arm/evidence/bridgevm-pc-variable-restore-live-20260830.md).

An eleventh fixed-sample live probe at exact host-runner code head
`7e749f814ef5f422eba8d57d6cc69e32103a9148` persisted that same validated
64 KiB backing to a private file and launched two separate BridgeVM processes
per lane. Each first process wrote and published the file without overwrite;
each second process opened the exact-size regular file without following a
final-component symbolic link, allocated fresh RAM and restored the payload in
a new HVF VM. All 20 lanes passed both stages and refused an additional
write-mode attempt against the existing path. This proves bounded
process-restart persistence, not host-reboot, power-loss or crash-recovery
semantics. See the
[updated exact-head receipt](../windows-arm/evidence/bridgevm-pc-variable-restore-live-20260830.md).

A twelfth fixed-sample live probe at exact code head
`afb0106c535863bce542a4ada0b3b589e081baea` connected the board-specific ECAM
routing to a minimal real HVF guest. Twenty independent processes each read the
host bridge plus all seven versioned endpoint identities at `00:00.0` through
`00:07.0`; all 20 lanes passed, for 160 validated stage-2 MMIO reads. The host
required the exact read width, destination register, IPA, order and identity.
This proves only guest-visible PCIe identity enumeration at the v1 ECAM
address, not firmware PciBus, BAR, DMA, interrupt, Block I/O, GOP, BDS or
Windows behavior. See the
[exact-head receipt](../windows-arm/evidence/bridgevm-pc-pcie-ecam-live-20260830.md).

A thirteenth fixed-sample live probe at exact code head
`b222143fccb3b75174d4e641d4d327c1cd8b98fa` moved the same eight identity
reads into the existing reset-to-DXE firmware image. Twenty independent
processes each created two fresh HVF VMs; all 20 lanes passed, for 40 firmware
boots and 320 validated ECAM reads while retaining the HOB, runtime-service,
variable, ACPI and SMBIOS checks. This proves direct firmware visibility only;
standard UEFI PCI host-bridge protocols, `PciBusDxe`, BARs, endpoint operation,
Block I/O, GOP, BDS and Windows remain open. See the
[exact-head receipt](../windows-arm/evidence/bridgevm-pc-firmware-pcie-ecam-live-20260830.md).

A fourteenth fixed-sample live gate at exact code head
`a67f24977278e5d0f8a54ba443af961d2ef69e13` replaced those direct probe reads
with a standard UEFI PCI path. The development firmware installed the
root-bridge I/O protocol and generic `PciBusDxe`; its probe required the PCI
driver binding, successful root-bridge connection, the enumeration-complete
marker and eight `EFI_PCI_IO_PROTOCOL` handles with the exact v1 identities.
All 20 independent lanes passed both separate-process boots, for 40 firmware
boots with variable-file restoration and zero failed lanes. BAR operation,
DMA, interrupts, Block I/O, GOP, BDS and Windows remain open. See the
[fixed-sample receipt](../windows-arm/evidence/bridgevm-pc-standard-uefi-pci-live-20260830.md).

A fifteenth fixed-sample live gate at exact code head
`bdc5e6e5c2aa5f2f9c9b59b12b7717e5a3966f41` extended that standard UEFI
path through the first real endpoint BAR. Generic `PciBusDxe` sized and
assigned the NVMe controller's 64-bit BAR0; the probe enabled memory decode
and bus mastering through `EFI_PCI_IO_PROTOCOL`, then read CAP and VS through
`PciIo->Mem.Read()`. All 20 independent lanes passed both separate-process
boots, for 40 firmware boots and 80 validated register reads at assigned base
`0x2000004000`. NVMe queue processing, DMA, interrupts, Block I/O, BDS and
Windows remain open. See the
[fixed-sample receipt](../windows-arm/evidence/bridgevm-pc-nvme-bar-live-20260831.md).

A sixteenth fixed-sample live gate at exact code head
`8f3dd9cce8e4cb6bdd9597b022e5000bca42b040` extended the assigned BAR path
through generic EDK2 `NvmExpressDxe`. The driver initialized the controller,
created the polling admin and I/O queues through guest DMA, discovered one
1 MiB namespace and published `EFI_BLOCK_IO_PROTOCOL`. All 20 independent
lanes passed both separate-process boots, for 40 firmware boots and 40 LBA0
reads returning the expected `BRIDGEVM` marker. MSI/MSI-X, write durability,
GOP, BDS, `ExitBootServices` and Windows remain open. See the
[fixed-sample receipt](../windows-arm/evidence/bridgevm-pc-nvme-block-live-20260831.md).

A seventeenth fixed-sample live gate at exact code head
`f6fbec0e5d5644d412614b1ca040449eac37c049` added a BridgeVM-owned BDS policy
and deterministic GPT/FAT16 ESP. After requiring all 12 PI architecture
protocols and signalling EndOfDxe, BDS connected the NVMe stack, found one
Simple File System handle and loaded `\EFI\BOOT\BOOTAA64.EFI` through standard
UEFI boot services. All 20 independent lanes reached code after a successful
`ExitBootServices`, each on the first attempt, with zero failed lanes. Windows
Boot Manager, Windows kernel entry, GOP, write durability and MSI/MSI-X remain
open. See the
[fixed-sample receipt](../windows-arm/evidence/bridgevm-pc-bds-exit-live-20260831.md).

## Firmware and Windows boundary

The firmware must publish ACPI and SMBIOS pointers through the standard UEFI
configuration table and provide the UEFI services and protocols Windows uses,
including block I/O, GOP, variables, time and reset. The ACPI platform is
hardware-reduced and describes GICv3, PCIe, TPM and the control-method power
button. PSCI owns vCPU startup, shutdown and reset coordination.

`BridgeVM boot-info v1` is a private firmware-internal handoff only. Windows
must not depend on it; after firmware processing, Windows sees standard UEFI,
ACPI, SMBIOS, PCIe and device interfaces.

The first independent firmware module is a DXE consumer in
`BridgeVmPcPkg`. It uses generic TianoCore `MdePkg` interfaces, validates the
fixed boot-info header, required ACPI v1 table set, FADT-to-DSDT link, SMBIOS
3.0 entry point and complete structure stream, then publishes the standard
UEFI ACPI 2.0 and SMBIOS 3.0 configuration-table GUIDs. It first built
reproducibly as a standalone AArch64 PE/COFF driver at exact BridgeVM code head
`49f2c8fdebb1c5c4bc445840fb7923afddfd099c`, the offline build pinned EDK2
`b03a21a63e3bd001f52c527e5a57feddb53a690b`, GCC 16.1.0 and iasl 20260408.
The resulting 12,288-byte development driver has SHA-256
`01f555aec886cf241f277c53ac2bf57d38fd064a8ae4b6508f4f4897802efccc`.
At exact code head `290f6b9c9c72c5659b53e1ae7ae10c8407cda720`, the
driver used unaligned-safe ACPI header reads, built with SHA-256
`16b3fdd6ede6d5aea14d26419351cf262ef358692fd28682dbbafe74c22438b5`,
and was integrated into the bounded DXE firmware volume. One HVF run proved
that DXE dispatched it and that both table pointers appeared in the EFI system
table. This does not establish the remaining UEFI services or protocols.

The package also contains a BridgeVM-owned reset entry built independently of
that DXE module. At exact BridgeVM code head
`5ed86f6bfa9bfb615bd2308178002710aa34f958`, pinned GCC 16.1.0 and GNU
binutils 2.46.1 produced a 92-byte entry at offset zero of a 64 MiB development
flash image with SHA-256
`af815a96240bb3cfd2ab19f6c853b70f609bdfca78f4a0885a08fb3ff9dbdf41`.
The entry executed once on HVF, but it is only a reset skeleton and does not
yet load the independently built DXE module.

The next exact-head tranche added a freestanding SEC C continuation to that
image. Its reset/SEC entry is 616 bytes, the complete development FD has
SHA-256 `745241de5a20d9240ec31c8000abb6f8ad04544a7ba7b9b4fe8c6f9b012cd890`,
and one HVF run passed the full fail-closed handoff validation. PI HOB
construction and DXE loading remain open.

The following exact-head tranche added the bounded PI HOB producer. Its
reset/SEC/HOB entry is 1,168 bytes, the complete development FD has SHA-256
`8db976249ff86c9613d0a13a0d811ee68a94ef90835d524deb451b927bc332d6`,
and one HVF run validated the exact five-HOB list in guest RAM. Firmware-volume
discovery and DXE loading remain open.

The next exact-head tranche added a development-only firmware volume, bounded
DXE IPL and fail-closed EL1 exception vector. Pinned tools produced a
6,144-byte entry and a 64 MiB FD with SHA-256
`57c134b8f3f42bb9bb020936d4d87926b0d6563bfa0339bb110996a6e4ed6da6`.
One HVF run entered generic DXE Core and dispatched the BridgeVM probe. UEFI
architectural protocols, boot/runtime-service completeness and Windows remain
open.

The following exact-head tranche added `PlatformTablesDxe` before the marker.
Pinned tools produced a byte-reproducible 64 MiB FD with SHA-256
`1227e77889f26cb19c0e2fef2b446b727c39fa652b863c21474692dd65128873`.
One HVF run proved that the EFI system table published the exact ACPI 2.0 and
SMBIOS 3 pointers. Architectural protocols, variables, boot manager, GOP,
block I/O and Windows remain open.

The next exact-head tranche added unmodified, BSD-2-Clause-Patent TianoCore
`RuntimeDxe` from the same pinned revision. Pinned tools produced a
byte-reproducible 64 MiB FD with SHA-256
`0a05d8ecb5bb96eb4088eda2f6c357aa044afb8cdbf92fd629c652da9dc89138`.
One HVF run proved the standard Runtime Architectural Protocol was installed
and that `CalculateCrc32` was callable, while ACPI and SMBIOS remained present.
Variable, time and reset implementations, virtual-address transition,
`ExitBootServices`, BDS, GOP, block I/O and Windows remain open.

The following exact-head tranche added generic `VariableRuntimeDxe` and a
BridgeVM probe for `GetVariable`, `SetVariable` and `QueryVariableInfo`. Pinned
tools produced a byte-reproducible 64 MiB FD with SHA-256
`37c659e4ec70050790607ab58ec8eb9066284f13eedccb50795cf4623c642172`.
A fixed 20-process live probe passed 20/20 two-VM recreations using fresh RAM
and one explicitly preserved in-memory vars backing per process. A subsequent
fixed 20-lane probe passed 20/20 across two separate BridgeVM processes and a
private file per lane. Host-reboot persistence, power-loss and crash recovery,
authenticated policy, time and reset services, virtual-address transition,
`ExitBootServices`, BDS, GOP, block I/O and Windows remain open.

The next exact-head tranche added eight fail-closed direct ECAM identity reads
to the same DXE probe. Pinned tools produced a byte-reproducible 64 MiB FD with
SHA-256
`352243b2ece7c3d0b0b0e97637e7a31aecc6a5fe8a34e389860ac2543a4e99f7`.
A fixed 20-process live probe passed 20/20, covering two fresh HVF VMs per
process and all eight identities per VM. This is not a replacement for the
standard UEFI PCI host-bridge and bus drivers; those remain open together with
BAR MMIO, DMA, interrupts, Block I/O, GOP, BDS and Windows boot.

The following exact-head tranche added a BridgeVM-owned PCI host-bridge
library, generic `PciHostBridgeDxe`/`PciBusDxe` and a standard PCI I/O probe.
Pinned tools produced a byte-reproducible 64 MiB FD with SHA-256
`42e294e45119d08a5a8d6b4f28b5de9b79872be9282d700832460977bbd8282b`.
The fixed `N=20` Studio tier passed 20/20 independent lane directories and 40
separate process boots. Each boot reported one root bridge, completed standard
PCI enumeration and exposed all eight exact `EFI_PCI_IO_PROTOCOL` identities.
This establishes enumeration only; BAR operation, DMA, interrupts, endpoint
queues, Block I/O, GOP, BDS and Windows boot remain open.

The next exact-head tranche connected the independent runtime's programmed
NVMe BAR0 to BridgeVM's NVMe register model and extended the standard PCI I/O
probe through `GetBarAttributes()`, PCI decode enable and `PciIo->Mem.Read()`.
Pinned tools produced a byte-reproducible 64 MiB FD with SHA-256
`f3296c4c0bd7900fa6c09519ab00c88a2bcd293846849b3c509d9d0076d9833b`.
The fixed `N=20` Studio tier passed 20/20 independent lane directories and 40
separate process boots. Every boot validated the exact 16 KiB BAR resource,
CAP `0x20020103ff`, NVMe 1.4 version `0x10400` and command `0x6`. DMA,
interrupts, NVMe queues, Block I/O, GOP, BDS and Windows boot remain open.

The next exact-head tranche added generic EDK2 `NvmExpressDxe` to the bounded
firmware volume and routed the independent runtime's NVMe doorbells, queue
memory and data buffers through live guest RAM. Pinned tools produced a
byte-reproducible 64 MiB FD with SHA-256
`5ccc1ce31e631312ba52408c0858c97f8fbec6a9f2b032ed84f1635984c79f5e`.
The fixed `N=20` Studio tier passed 20/20 independent lane directories and 40
separate process boots. Every boot found one namespace with 512-byte blocks
and last block 2047, then read LBA0 through `EFI_BLOCK_IO_PROTOCOL` and
returned marker `BRIDGEVM`. MSI/MSI-X, write durability, GOP, BDS,
`ExitBootServices` and Windows boot remain open.

The following exact-head tranche added the BDS architectural protocol, a
removable-media boot policy and a deterministic ESP containing an AArch64 UEFI
application. Pinned tools produced a byte-reproducible development FD with
SHA-256
`9bf4152f31bf304a384341ee8f9fce7f9d2fc890b9302a19935e107596575849`
and ESP image with SHA-256
`a49be97db44c0d68b3382f3b1e46eba2fc7a3b12bcba14c1ec720f0511b71979`.
The fixed `N=20` Studio tier passed 20/20 independent disk-and-vars lanes.
Every boot required architecture mask `0xfff`, loaded `BOOTAA64.EFI` from the
NVMe filesystem, obtained a version-1 memory map and reached post-
`ExitBootServices` stage 11 on its first attempt. Windows Boot Manager, Windows
kernel entry, GOP, write durability and MSI/MSI-X remain open.

## Implementation gates

| Gate | State |
|---|---|
| Versioned identity, address map and overlap tests | implemented, static only |
| Minimal memory layout, vars flash, UART, RTC and PCIe ECAM runtime | implemented, host-unit only |
| Guest PCIe identity enumeration at the v1 ECAM | fixed `N=20` minimal-guest, direct-firmware and standard UEFI `PciBusDxe` probes passed for the host bridge and all seven endpoints |
| Standard UEFI NVMe BAR0 sizing, assignment and MMIO | fixed `N=20` passed across 40 separate process boots; DMA, interrupts, queues and Block I/O open |
| Standard UEFI NVMe polling queues, guest DMA and Block I/O read | fixed `N=20` passed across 40 separate process boots and 40 LBA0 reads; MSI/MSI-X, write durability, BDS and Windows open |
| UEFI BDS, ESP image loading and `ExitBootServices` | fixed `N=20` passed across 20 independent disk-and-vars boots; Windows Boot Manager, kernel entry and GOP open |
| Host GIC geometry validation and bounded placement probe | live single-run placement passed |
| HVF RAM mapping and architected-timer PPI delivery | live single-run passed; firmware and device SPI integration open |
| Boot-info v1 mapping and EL1 pointer traversal | live single-run passed; firmware validation and consumption passed separately |
| ACPI/SMBIOS DXE consumer | reproducible firmware-volume build and live single-run publication passed |
| Reset vector at GPA zero | live single-run passed; SEC, PI HOB and DXE-dispatch continuations passed separately |
| Freestanding SEC validation | live single-run passed; bounded PI HOB and DXE-dispatch continuations passed separately |
| Bounded PI HOB construction | live single-run passed; firmware-volume and DXE-dispatch continuation passed separately |
| Generic DXE Core and BridgeVM marker dispatch | live single-run passed; all 12 BDS prerequisite architecture protocols and boot manager subsequently passed fixed `N=20` |
| Runtime Architectural Protocol and CRC32 service | live single-run passed; virtual-address transition and complete runtime services open |
| UEFI variable services across VM and process recreation | fixed `N=20` in-memory VM-recreate and fixed `N=20` file-backed separate-process probes passed with fresh RAM; host reboot, power-loss and crash recovery open |
| Independently built and audited UEFI firmware | open |
| UEFI ACPI/SMBIOS handoff | live single-run publication passed; fixed-sample and Windows consumption open |
| UEFI GOP and block-I/O handoff | standard NVMe Block I/O read plus BDS/ESP consumption passed fixed `N=20`; GOP and Windows consumption open |
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
