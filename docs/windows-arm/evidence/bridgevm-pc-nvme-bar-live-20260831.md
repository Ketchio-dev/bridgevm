# BridgeVM PC standard UEFI NVMe BAR access, fixed N=20 (2026-08-31)

This is development evidence for the experimental BridgeVM Virtual ARM PC. It
does not change the shipping Windows board or promote the product state.

## Result

Studio tier `t11-bridgevm-pc-nvme-bar` job
`20260831-001851-66185-17681` ran on exact tested code head
`bdc5e6e5c2aa5f2f9c9b59b12b7717e5a3966f41` and completed:

| Field | Result |
|---|---:|
| required lanes | 20 |
| attempted lanes | 20 |
| passing lanes | 20 |
| failed lanes | 0 |
| separate BridgeVM process boots | 40 |
| validated NVMe register reads | 80 |
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
  enumeration, one PCI driver binding and all eight exact endpoint identities;
- 40 `nvme_bar_reads=2` results at BAR0 base `0x2000004000`, length 16 KiB,
  CAP `0x20020103ff`, version `0x10400` and PCI command `0x6`;
- 20 complete two-process lane markers; and
- zero matches for `Illegal resource`, `context error reported`, `unexpected`,
  `probe failed`, `FAIL:`, out-of-aperture or unhandled-MMIO errors.

The firmware did not read a hard-coded guest address. It found the NVMe
`EFI_PCI_IO_PROTOCOL` handle at `00:01.0`, obtained BAR0's standard ACPI
resource descriptor with `GetBarAttributes()`, required a 64-bit memory
resource of exactly 16 KiB inside a v1 non-prefetchable aperture, enabled
Memory Space and Bus Master attributes, checked PCI command readback and used
`PciIo->Mem.Read()` for CAP and VS. Each read exited to BridgeVM's independent
board runtime, which resolved the programmed 64-bit BAR through its PCIe model
and dispatched the offset to BridgeVM's NVMe controller register model.

## Sealed identities

| Artifact | SHA-256 |
|---|---|
| public receipt | `b7a37c663e549fef29c3e5aecfe0565ba7f864b8f811f64fa6fdf30fbd41841f` |
| complete retained receipt | `e3c4e5bbae12abfb71f1c2e8e20bab19ded5acff8672fc757d0643ccb04f1140` |
| summary | `8423027d6f6b7f34a43b6732803082d25b965ad806d29d39f329af5b91a183fe` |
| tier run log | `79c810b30881d706c8c0471be7b44339533ea5033d6bcaadc4f4f289285f7a94` |
| aggregate of 20 lane logs | `754ea015e7ecaaea4903348a12637c3b32f256bbd3f0dad081ed5bae5eff9c3d` |
| ad-hoc-signed probe runner | `e44a6370eeb151ffd16be53c4dbd76366d9d7214bff74d9552d260faa08fff93` |
| development FD | `f3296c4c0bd7900fa6c09519ab00c88a2bcd293846849b3c509d9d0076d9833b` |
| firmware volume | `f25ad27c72086de1ab3e4d74f54f41c5280c26bc055695c53741525b95322ba4` |
| firmware build receipt | `12c5248a97c68c69a9d82470598c5735bb652a71bcb61bf62c2cbd103e3c9d60` |

The reproducible development build pins EDK2 commit
`b03a21a63e3bd001f52c527e5a57feddb53a690b`, GCC 16.1.0 and GNU binutils
2.46.1. Its build receipt records source-tree SHA-256
`4906818373a37d060eaf019f2a722a70d1e0afd1728b002af27938b15ebccd40`,
PCI-stack receipt SHA-256
`54b29e489b15cfd8b1b8125eb8448831e322c5cfabf2af3a16f78e1ab90126a8`
and DXE probe SHA-256
`860d62953e79f54365708058eb8b3d72bfef0558f776488786428e90f9508cb8`.
The retained public receipt is
[`bridgevm-pc-nvme-bar-receipt-20260831.json`](bridgevm-pc-nvme-bar-receipt-20260831.json).

GitHub-hosted CI run `33356380313` and Security and quality run
`33356381566` both completed successfully at the exact tested code head.

## Failed submission retained

Job `20260831-001704-65506-21044` failed before creating a worktree because
the worker mirror had not fetched the new exact commit. It performed zero
lanes and supplies no positive evidence. The mirror then fetched that exact
commit and the successful job above created a new worktree; the failed job was
not reused or rewritten.

## Claim boundary

This proves fixed-sample standard UEFI sizing, assignment, decode enable and
real read access for the independent board's NVMe BAR0 while retaining the
existing HOB, runtime-service, variable, ACPI, SMBIOS and PCI-enumeration
checks.

It does **not** prove DMA, NVMe admin or I/O queues, MSI/MSI-X delivery, a
namespace, UEFI Block I/O, GOP, BDS, `ExitBootServices`, Windows installation
or Windows boot. The development FD and ad-hoc runner signature are not
production signing evidence. The independent board remains experimental, the
QEMU Compatibility Engine remains available, A9 remains OPEN, and B4 remains
separately PROVEN at fixed N=20 with p95 243 ms.
