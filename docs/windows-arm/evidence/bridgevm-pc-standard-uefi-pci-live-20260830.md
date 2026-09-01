# BridgeVM PC standard UEFI PCI enumeration, fixed N=20 (2026-08-30)

This is development evidence for the experimental BridgeVM Virtual ARM PC. It
does not change the shipping Windows board or promote the product state.

## Result

Studio tier `t9-bridgevm-pc-pci` job
`20260830-233459-98763-23619` ran on exact tested code head
`a67f24977278e5d0f8a54ba443af961d2ef69e13` and completed:

| Field | Result |
|---|---:|
| required lanes | 20 |
| attempted lanes | 20 |
| passing lanes | 20 |
| failed lanes | 0 |
| separate BridgeVM process runs | 40 |
| receipt `pass` | `true` |
| host | `Mac17,9`, macOS 26.5 |

Every lane used its own output directory and `BridgeVmPcVars.fd`. The first
process created and wrote that lane's fail-closed variable file; the second
process started a fresh HVF VM with fresh RAM and restored the exact variable.
The receipt's aggregate variable-file hash is
`2d89d90e7b6337e1194211112bbcc83311d4d1ef3df9a38939f993c41dfdec55`.

The 20 retained lane logs contain exactly:

- 20 `process_mode=written` and 20 `process_mode=restored` results;
- 40 results with `pcie_functions=8`, `pci_root_bridges=1`,
  `pci_enumeration_complete=1` and `pci_driver_bindings=1`;
- 40 `pci_supported_status=0` and `pci_connect_status=0` results;
- 40 successful firmware-stage markers and 20 complete two-process lane
  markers; and
- zero matches for `Illegal resource`, `context error reported`, `unexpected`,
  `probe failed` or `FAIL:`.

The probe did not count direct ECAM loads as standard enumeration. It located
the root-bridge I/O protocol, found the generic PCI bus driver binding, called
its `Supported()` path, connected the root bridge, required the standard PCI
enumeration-complete marker, then enumerated eight `EFI_PCI_IO_PROTOCOL`
handles. For each handle it obtained segment/bus/device/function and read the
exact v1 identity through `PciIo->Pci.Read`. The host-side result parser
required all eight versioned identities at `00:00.0` through `00:07.0`.

## Sealed identities

| Artifact | SHA-256 |
|---|---|
| public receipt | `d14beb276e80c306d5785b28894d4a1611fc34060db4f19663a558e59217cb91` |
| complete receipt | `405c15aeb6b9e9f640cdc6bc5be83e48f3f1c25173a3cd31a93499d6b2b65464` |
| summary | `8423027d6f6b7f34a43b6732803082d25b965ad806d29d39f329af5b91a183fe` |
| tier run log | `8886ae6176a19353c9c3c64815148348b9f5ba022a49157289100c5da20ad421` |
| ad-hoc-signed probe runner | `616e5cb60a8d640266fc9dd91a872d8ec8526fe4ac5b79f7091794402ec22793` |
| development FD | `42e294e45119d08a5a8d6b4f28b5de9b79872be9282d700832460977bbd8282b` |
| firmware volume | `0b093cc672914ecf7b2b842b83d8325ffc870d1e314993cab963f78a01fcf78e` |
| firmware build receipt | `2d766ec8e0fb0faea4810d9d733689f35dae68e2dbb30e3e575f3321fbf732f2` |

The reproducible development build pins EDK2 commit
`b03a21a63e3bd001f52c527e5a57feddb53a690b`, GCC 16.1.0 and GNU binutils
2.46.1. The build receipt records source-tree SHA-256
`22a0c59b02764ff3cd11dc4f34927a695c8444bbf7e6b9b26fae6459986ff71e`,
PCI-stack receipt SHA-256
`54b29e489b15cfd8b1b8125eb8448831e322c5cfabf2af3a16f78e1ab90126a8`
and probe SHA-256
`3258c4efc888efa2415be86756af4fd9fcbd5ac874a89df838ce1c89c87b31e0`.
The retained public receipt is
[`bridgevm-pc-standard-uefi-pci-receipt-20260830.json`](bridgevm-pc-standard-uefi-pci-receipt-20260830.json).

GitHub-hosted CI run `33354295207` and Security and quality run `33354296401`
both completed successfully at the exact tested code head.

## Failed evidence retained

The first two submissions, `20260830-232641-80380-14693` and
`20260830-232641-80387-1489`, failed before a worktree existed because the
worker clone had not fetched commit `c5cc6c6ca779281b950620026e48c937838dac64`.
They have no positive measurement.

After fetching that commit, job `20260830-232731-80750-8893` reached one lane
but was cancelled and retained as a failure. Its restricted LaunchAgent `PATH`
did not contain `rg`; shell conditionals around the missing command allowed
prohibited-reference scans to be skipped. Exact head `a67f249…` replaced those
checks with one portable, fail-closed scanner that distinguishes match, clean
scan and scanner error. A complete firmware build under the restricted worker
environment passed that scanner and reproduced the same FD hash before the
fixed-sample job above was submitted. The failed job is not counted as
evidence for this result.

## Claim boundary

This proves fixed-sample discovery of the host bridge and all seven endpoints
through the standard UEFI PCI host-bridge and `PciBusDxe` protocol path while
retaining the existing HOB, runtime-service, variable, ACPI and SMBIOS checks.

It does **not** prove BAR sizing or assignment correctness, endpoint BAR MMIO,
DMA, MSI/MSI-X delivery, NVMe or virtio queues, UEFI Block I/O, GOP, BDS,
`ExitBootServices`, Windows installation or Windows boot. The development FD
is not production-signed firmware. The independent board remains
experimental, the QEMU Compatibility Engine remains available, A9 remains
OPEN, and B4 remains separately PROVEN at fixed N=20 with p95 243 ms.
