# BridgeVM Virtual ARM PC: firmware PCIe ECAM visibility (2026-08-30)

## Evidence rank and result

This is a fixed-`N=20` live engineering receipt on real Apple silicon, not a
product release criterion. At exact BridgeVM code head
`b222143fccb3b75174d4e641d4d327c1cd8b98fa`, 20 independent ad-hoc-signed
runner processes each created two fresh HVF VMs. All 20 process lanes passed.

In every VM, the existing DXE probe performed eight volatile 32-bit reads from
the BridgeVM Virtual ARM PC v1 PCIe ECAM aperture. It required the host bridge
at `00:00.0` and the seven versioned board functions at `00:01.0` through
`00:07.0` to return these identities:

| BDF | Role | Identity DWORD |
|---|---|---:|
| `00:00.0` | host bridge | `0x00081b36` |
| `00:01.0` | system storage | `0x00101b36` |
| `00:02.0` | USB input | `0x000d1b36` |
| `00:03.0` | installer media | `0x10011af4` |
| `00:04.0` | network | `0x10411af4` |
| `00:05.0` | display | `0x10501af4` |
| `00:06.0` | guest agent | `0x10431af4` |
| `00:07.0` | audio | `0x26688086` |

An identity mismatch returns `EFI_COMPROMISED_DATA` before the probe's HVC.
The host runner accepts only instruction-syndrome-valid data aborts inside the
v1 ECAM range, routes them through `BridgeVmPcPlatform::on_mmio`, and requires
exactly eight MMIO exits before accepting the HVC. Its independent parser then
requires the count and all eight firmware-observed identities in guest RAM.

The fixed sample therefore covered 40 recreated HVF VMs and 320 validated
firmware ECAM reads. Each process used one isolated in-memory variable backing
for its two VMs; no disk or vars state was shared between process lanes. Every
lane reported the bounded result:

```text
BridgeVM Virtual ARM PC variable restore probe: PASS
board=com.ketchio.bridgevm.virtual-arm-pc abi=1
... smbios=0x2600c000 pcie_functions=8 ...
firmware_sha256=352243b2ece7c3d0b0b0e97637e7a31aecc6a5fe8a34e389860ac2543a4e99f7
LIVE PROOF: a recreated HVF VM restored the non-volatile UEFI variable from the preserved vars backing
```

## Reproducible identities

The read-only local seal is
`bridgevm-pc-firmware-pcie-b222143fccb3b75174d4e641d4d327c1cd8b98fa`.

| Item | SHA-256 |
|---|---|
| complete 20-lane live log | `a315b36511f5524c5821b6eeee6fa45c1d0e77ce7602c06031be76775ed06495` |
| result record | `07290683d1ca2717c08a6920a1dbb3f4050b245c88323d2198f58e3a4d2068fe` |
| nine-artifact content manifest | `e6e6ec530772a45cf57d8cfb1d11a16e5ceec63e5beeff513abad6ee8e2edfe6` |
| ad-hoc-signed exact-head runner | `660d9e3103e0ea0a4c1a451757e1c40cd7adfa04defb78c1768cf779cb72f727` |
| retained runner script | `c79487f18a2c91c7295f0bbef13b0466dbbb09dda6b76fc88b04f2a6f1542f89` |
| exact Git source archive | `a113a8ed39d61e4f5afe39880b596c024dd80b7a3d9a288fab336089c1ffade5` |
| reproducible build log | `3cbed640c97cb2de5f1f9eee48dab6ee2dc9bca892f9a49e72cff5d20e68e541` |
| build receipt | `88b7a1f214b7446a396df53c6de96543e06ea5e3bd8cd7159d4aa0bc02beea8d` |
| 64 MiB development FD | `352243b2ece7c3d0b0b0e97637e7a31aecc6a5fe8a34e389860ac2543a4e99f7` |
| development firmware volume | `a46c7561bb18104fe30ea91594b25fdc30b413eabe91fc8d1256d68c37fa06fd` |

The build pins EDK2 commit
`b03a21a63e3bd001f52c527e5a57feddb53a690b`, GCC 16.1.0 and GNU
binutils 2.46.1. The DXE probe PE/COFF digest is
`de2d1a213c55f0134f7ab22d206ec6ac6fbc1d4453ad8bfb61980bc0aaea5778`.
The runner carries only the Hypervisor.framework entitlement and is a local
probe, not a redistributable product binary.

## Exact-head deterministic receipts

GitHub-hosted CI run `33348860564` passed its complete matrix at the exact code
head, and GitHub-hosted Security and quality run `33348861765` passed its
supply-chain, fuzz, concurrency, claim and live-gate-policy checks.

Studio `t0-check` job `20260830-215443-71549-299` ran the complete
`scripts/check-project.sh` at the same SHA. All code and test steps passed,
including 2,122 reachable tests, the independent-firmware boundary and all
three Swift shim suites. Its sole failed step was the intentionally stale A11
capability registry before this documentation-only seal. The check-log
SHA-256 is
`0e0df56c8f8ebd7bc4661b94427356394ab51d2b6207c708cd289e5ab0552e7a`.
The preceding job `20260830-215349-71258-23787` failed before tests because the
worker clone had not fetched the exact commit; it has no receipt and remains
retained as a failed submission.

## Relationship to the preceding probe

The earlier exact-head receipt at `afb0106c535863bce542a4ada0b3b589e081baea`
proved that a minimal EL1 guest could reach the eight endpoint identities
through the board-specific ECAM routing. This receipt advances that boundary
by executing the reads from the existing reset-to-DXE firmware image while
retaining its HOB, runtime-service, variable, ACPI and SMBIOS validations.

The implementation deliberately uses direct ECAM loads in the bounded DXE
probe. It does not install the standard UEFI PCI host-bridge protocols and does
not dispatch generic `PciBusDxe`.

## Claim boundary

This does **not** prove standard UEFI PCI enumeration, BAR sizing or
assignment, endpoint BAR MMIO, DMA, MSI/MSI-X delivery, NVMe or virtio queues,
UEFI Block I/O, GOP, BDS, Windows installation or Windows boot. It does not
change the shipping Windows board. The independent board remains experimental.
A9 remains OPEN, and B4 remains separately PROVEN at 20/20 with p95 243 ms.
