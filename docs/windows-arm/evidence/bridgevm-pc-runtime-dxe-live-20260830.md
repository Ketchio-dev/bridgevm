# BridgeVM Virtual ARM PC: RuntimeDxe installs its architectural protocol (2026-08-30)

## Evidence rank and result

This is a **live single-run** receipt, not a fixed-sample product gate. At exact
BridgeVM code head `0890d838e1dbc20f0ab393646023d2d8014a9b91`, the
development-only BridgeVM firmware dispatched the pinned generic TianoCore
`RuntimeDxe` on a real `Mac17,9` host running macOS 26.5. A BridgeVM probe whose
DXE dependency expression requires the standard Runtime Architectural Protocol
then called the installed boot-service CRC32 function and returned:

```text
hvc_iss=0x0 args=[100002000, 9, 16]
BridgeVM Virtual ARM PC runtime DXE probe: PASS
board=com.ketchio.bridgevm.virtual-arm-pc abi=1
reset_vector=0x0 firmware_size=0x4000000 ram=0x100000000
sec_result=1 hob_count=7 hob_list_gpa=0x100004000 hob_list_size=272 dxe_result=9 system_table=0x11ffc0018 runtime_services=0x11ffcff18 runtime_protocol=0x11ff94000 runtime_crc32=0x3f6f728d configuration_entries=7 acpi=0x26001000 smbios=0x2600c000 result_gpa=0x100001000 boot_info=0x26000000
firmware_sha256=0a05d8ecb5bb96eb4088eda2f6c357aa044afb8cdbf92fd629c652da9dc89138
LIVE PROOF: RuntimeDxe installed its architectural protocol and callable CRC32 service
binary_sha256=ce8d31bb338dc1943ecba0953996a7f99264042d9b0a97ffacff1d7a342a0293
PASS: BridgeVM Virtual ARM PC installed RuntimeDxe and retained its platform tables
```

The command ran from a detached worktree at the exact SHA:

```sh
BRIDGEVM_HVF_ALLOW_LIVE_BRIDGEVM_PC_DXE_ENTRY=1 \
  tests/integration/bridgevm-pc-dxe-entry-live-opt-in-smoke.sh
```

The complete command output has SHA-256
`4d3acf077e95dbbd2ba4f294ec8f089dff8950d75639b6f1bf0f739a6ecb799e`.
The ad-hoc-signed debug example carried only the Hypervisor.framework
entitlement and is a local probe, not a redistributable product binary.

## What ran

The firmware volume contained, in order, the fixed-rebased generic DXE Core,
generic `RuntimeDxe`, BridgeVM `PlatformTablesDxe`, and the BridgeVM probe.
`RuntimeDxe` is unmodified code from pinned TianoCore commit
`b03a21a63e3bd001f52c527e5a57feddb53a690b` under
BSD-2-Clause-Patent. Its exact firmware file GUID is
`B601F8C4-43B7-4784-95B1-F4226CB40CEE`.

The probe's dependency expression names `gEfiRuntimeArchProtocolGuid`; it is no
longer an unconditional `TRUE` marker. Once dispatched, the probe fail-closed
required:

- non-null EFI boot- and runtime-services tables;
- the standard runtime-services table signature;
- non-null `SetVirtualAddressMap`, `ConvertPointer`, and `CalculateCrc32`
  entries;
- a successful `LocateProtocol` for the Runtime Architectural Protocol; and
- `CalculateCrc32("BridgeVM RuntimeDxe v1") == 0x3f6f728d`.

It wrote the stage last, after every check and service call passed. The host
did not accept that stage alone: it bounded every returned pointer to mapped
guest RAM, required the standard boot- and runtime-services signatures,
required all three function pointers to match their service-table slots, and
independently revalidated the exact ACPI 2.0 and SMBIOS 3 configuration-table
pointers from the preceding tranche.

## Reproducible build

Two consecutive complete builds compared the FD, FV and JSON build receipt
byte-for-byte equal. The pinned identities are:

| Artifact | SHA-256 |
|---|---|
| 6,144-byte reset/SEC/HOB/IPL vector | `a8d8a79279903253dd7dcc4d34a43aa5c00ac597cf45db613c9d23f03c69ddba` |
| fixed-rebased DXE Core | `b4ca5c00ef7e1b4104776005fe3c07978c78e39d92d2f035cfa72edabdf77d10` |
| generic RuntimeDxe | `1ec9344d12805f32ae7e68a1a0d755ad00f5b7b59414a75cebfaaaae19ffab75` |
| RuntimeDxe dependency expression | `557c754d26e2667287367a856ea5fcd584f35ab796d24a6a875d1648a4637d23` |
| BridgeVM PlatformTablesDxe | `16b3fdd6ede6d5aea14d26419351cf262ef358692fd28682dbbafe74c22438b5` |
| BridgeVM runtime probe | `d0cf902d9cfec6456c23a09b10d5aca49010a88e30ce4e1ed888cfa3f54e8256` |
| combined firmware volume | `d7b88cf02b786e4cdefec0190d863eb949470c9d221d43f4064b535489dc61c9` |
| complete 64 MiB development FD | `0a05d8ecb5bb96eb4088eda2f6c357aa044afb8cdbf92fd629c652da9dc89138` |
| BridgeVmPcPkg source tree | `9dbe887c037c402501e57e045428a88ede65984479a5a697cfae208ba1b32faf` |
| JSON build receipt | `a32609d1bf89fee77e11f050c4a04d37ec626dd2292f91f0ca11a549e1844375` |

The earlier reset/SEC/HOB-only FD remained byte-identical at
`8db976249ff86c9613d0a13a0d811ee68a94ef90835d524deb451b927bc332d6`
and its bounded HVF regression passed again after this integration.

The first Studio t0-check submission,
`20260830-190447-3212-22994`, failed before running tests because the detached
worker clone had not fetched the new commit. That infrastructure failure is
retained and is not supporting evidence. The worker clone was fetched without
changing the submitted SHA, and replacement job
`20260830-190616-5198-31792` then ran the complete
`scripts/check-project.sh` against the same exact head. Every code and test step
passed, including 2,114 reachable tests and the independent-firmware boundary;
its sole failure was the intentionally stale capability registry before the
subsequent documentation-only A11 seal. The check-log SHA-256 is
`f25de71b59f17e328f0c58b0a58016a6b937c4c6d19cd219ab20a0b689c0a631`.

GitHub-hosted CI run `33340894307` passed the repository matrix at the exact
code head, and Security and quality run `33340895550` passed its supply-chain,
fuzz, Loom, graphics-claim and live-gate policy jobs.

## Claim boundary

This result proves one reset-to-DXE run in which generic `RuntimeDxe` installed
the Runtime Architectural Protocol and supplied a callable CRC32 boot service,
while the previously proven ACPI and SMBIOS pointers remained installed.

It does **not** prove that `SetVirtualAddressMap` or `ConvertPointer` succeeds;
complete runtime-variable, time or reset services; `ExitBootServices`; BDS;
GOP; block I/O; Windows installation; or Windows boot. Device, security and
fixed-sample reliability gates for the independent board remain open. The
shipping Windows engine retains its current board contract, A9 remains OPEN,
and the separate B4 result remains 20/20 with p95 243 ms.
