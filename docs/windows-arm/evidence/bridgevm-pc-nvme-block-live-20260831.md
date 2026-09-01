# BridgeVM PC standard UEFI NVMe Block I/O, fixed N=20 (2026-08-31)

This is development evidence for the experimental BridgeVM Virtual ARM PC. It
does not change the shipping Windows board or promote the product state.

## Result

Studio tier `t12-bridgevm-pc-nvme-block` job
`20260831-010811-74407-18695` ran on exact tested code head
`8f3dd9cce8e4cb6bdd9597b022e5000bca42b040` and completed:

| Field | Result |
|---|---:|
| required lanes | 20 |
| attempted lanes | 20 |
| passing lanes | 20 |
| failed lanes | 0 |
| separate BridgeVM process boots | 40 |
| validated NVMe register reads | 80 |
| standard UEFI Block I/O handles per boot | 1 |
| successful LBA0 reads | 40 |
| receipt `pass` | `true` |
| host | `Mac17,9`, macOS 26.5 |

Each lane had its own directory and `BridgeVmPcVars.fd`. Its written and
restored stages ran in separate BridgeVM processes with fresh RAM. All 20
variable files remained byte-identical across restoration; their independently
recomputed aggregate SHA-256 matches the receipt value
`2d89d90e7b6337e1194211112bbcc83311d4d1ef3df9a38939f993c41dfdec55`.

The 20 retained lane logs contain exactly:

- 20 `process_mode=written` and 20 `process_mode=restored` results;
- 40 standard PCI results with one root bridge, completed `PciBusDxe`
  enumeration, two PCI driver bindings and all eight exact endpoint identities;
- 40 assigned 16 KiB NVMe BAR results with CAP `0x20020103ff`, version
  `0x10400` and PCI command `0x7`;
- 40 standard UEFI Block I/O results with one present namespace, 512-byte
  blocks, last block 2047, one successful read and LBA0 marker `BRIDGEVM`;
- 20 complete two-process lane markers; and
- zero matches for `Illegal resource`, `context error reported`, `unexpected`,
  `probe failed`, `FAIL:`, out-of-aperture or unhandled-MMIO errors.

The firmware volume contains generic EDK2 `NvmExpressDxe`, scheduled by a
standard true DXE dependency. After generic `PciBusDxe` enumeration, the driver
bound to the independent board's `00:01.0` endpoint. Its ordinary controller
initialization used the programmed BAR, guest-memory queue addresses and PCI
DMA mapping to issue Identify and queue-management commands, discover the one
1 MiB namespace and publish `EFI_BLOCK_IO_PROTOCOL`. The probe then called
`ReadBlocks()` for LBA0 and required the eight-byte `BRIDGEVM` marker returned
from BridgeVM's NVMe backing medium. No hard-coded completion or probe-side
copy substitutes for that driver path.

## Sealed identities

| Artifact | SHA-256 |
|---|---|
| public receipt | `b69ed5e611956bd390664ba0f6297e9800318168e3f6d8b3c638901e7b8a3a21` |
| complete retained receipt | `bfe89f79d36e4733dbfcda084825c3e67218c1d6de465b21b77dadaebe6a600b` |
| summary | `8423027d6f6b7f34a43b6732803082d25b965ad806d29d39f329af5b91a183fe` |
| tier run log | `0268c52706528d494bea8bb531129e6497fe1cd256f17f951b594e3de066137f` |
| aggregate of 20 lane logs | `dc986159a482988bb1f2ea9ef75d4678aaa02fab6a50556750f3c2502d9b980c` |
| ad-hoc-signed probe runner | `ed23573689425cfe7f10cd64b34fad31de1b19799b4b51f2c44a27aa28a3b6e2` |
| development FD | `5ccc1ce31e631312ba52408c0858c97f8fbec6a9f2b032ed84f1635984c79f5e` |
| firmware volume | `ad51ab55c1d1f2d8db9171fcc38703a0e440dca0c572025a82c988a7a6310ded` |
| firmware build receipt | `08e3ed9c34d5643817f605e5b3a229fbb55581cf4511ca4a893841ec1835dec5` |

The reproducible development build pins EDK2 commit
`b03a21a63e3bd001f52c527e5a57feddb53a690b`, GCC 16.1.0 and GNU binutils
2.46.1. Its build receipt records source-tree SHA-256
`3cc821a984416e1d97fa41aaaedc088b2219c34edd9c8c7f4a4bc963e4c58b4b`,
PCI-stack receipt SHA-256
`ffb9b3efc92464ed3e9aa62d9f74fe80dc9c94a1787312a8a934a48fd91241f4`
and DXE probe SHA-256
`4c2f2901aaeb61f1dc69566d93fe7c18a06617ba6b95c7a7f94b0f606c31a88b`.
The retained public receipt is
[`bridgevm-pc-nvme-block-receipt-20260831.json`](bridgevm-pc-nvme-block-receipt-20260831.json).

GitHub-hosted CI run `33359438554` and Security and quality run
`33359440008` both completed successfully at the exact tested code head.

## Failed submissions retained

Job `20260831-010036-39064-8364` failed its first lane because the generic NVMe
driver had not been scheduled by the DXE dispatcher. The log reported one
driver binding and `EFI_NOT_FOUND`; it supplies no positive Block I/O evidence.

Job `20260831-010549-67351-25397` also retained a failed receipt after one lane.
Both boots in that lane actually completed Block I/O and read the expected
marker, but the gate still required the earlier BAR-only PCI command value
`0x6` instead of the standard driver's observed `0x7`. That parser failure was
not rewritten as a pass. The successful job above used a new exact commit and
new worktree, and ran the full fixed sample.

## Claim boundary

This proves the independent board's polling NVMe admin-queue, I/O-queue, guest
DMA, namespace discovery and standard UEFI Block I/O read path at fixed N=20,
while retaining the existing HOB, runtime-service, variable, ACPI, SMBIOS and
PCI-enumeration checks.

It does **not** prove MSI/MSI-X delivery, write durability, a production disk
backend, GOP, BDS, `ExitBootServices`, Windows installation or Windows boot.
The development FD and ad-hoc runner signature are not production signing
evidence. The independent board remains experimental, the QEMU Compatibility
Engine remains available, A9 remains OPEN, and B4 remains separately PROVEN at
fixed N=20 with p95 243 ms.
