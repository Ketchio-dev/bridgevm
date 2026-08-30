# BridgeVM Virtual ARM PC: DXE publishes ACPI and SMBIOS (2026-08-30)

## Evidence rank and result

This is a **live single-run** receipt, not a fixed-sample product gate. At exact
BridgeVM code head `290f6b9c9c72c5659b53e1ae7ae10c8407cda720`, the
development-only BridgeVM firmware entered a pinned generic DXE Core on a real
`Mac17,9` host running macOS 26.5. The core dispatched the BridgeVM-owned
platform-table driver before the marker probe. An independent host parser then
walked the EFI system table and required the exact ACPI 2.0 and SMBIOS 3
configuration-table entries:

```text
hvc_iss=0x0 args=[100002000, 8, 1004131d8]
BridgeVM Virtual ARM PC platform-table DXE probe: PASS
board=com.ketchio.bridgevm.virtual-arm-pc abi=1
reset_vector=0x0 firmware_size=0x4000000 ram=0x100000000
sec_result=1 hob_count=7 hob_list_gpa=0x100004000 hob_list_size=272 dxe_result=8 system_table=0x11ffc0018 configuration_entries=7 acpi=0x26001000 smbios=0x2600c000 result_gpa=0x100001000 boot_info=0x26000000
firmware_sha256=1227e77889f26cb19c0e2fef2b446b727c39fa652b863c21474692dd65128873
LIVE PROOF: BridgeVM published ACPI 2.0 and SMBIOS 3 through the EFI system table
binary_sha256=f47456cef8ef385e55352f7ebe075233cb1cfe213e4e8a945f726bfd112f8b04
PASS: BridgeVM Virtual ARM PC published its platform tables through DXE
```

The exact command was:

```sh
BRIDGEVM_HVF_ALLOW_LIVE_BRIDGEVM_PC_DXE_ENTRY=1 \
  tests/integration/bridgevm-pc-dxe-entry-live-opt-in-smoke.sh
```

The host had booted at 2026-08-30 11:34:29 EDT. The ad-hoc-signed debug
example carried only the Hypervisor.framework entitlement and had post-signing
SHA-256
`f47456cef8ef385e55352f7ebe075233cb1cfe213e4e8a945f726bfd112f8b04`.
It is a local probe, not a redistributable product binary.

GitHub-hosted CI run `33339437458` and Security and quality run
`33339437543` both passed at the exact code head. Studio t0-check job
`20260830-183546-76087-30374` ran the complete `scripts/check-project.sh`:
every code and test step passed, including 2,112 reachable tests and the
independent-firmware boundary. Its sole failure was the intentionally stale
capability registry before the subsequent documentation-only A11 seal. The
check-log SHA-256 was
`83f6f79e2205e649fd1f114fdb3ba78164596d2e36a23e7fa1604ee7fffad7cd`.

The preceding queue job `20260830-183251-75092-23124` failed before any test
ran because the worker's detached repository had not fetched the sealed
commit. That infrastructure failure is retained, was corrected by fetching
the branch, and is not supporting evidence for this result.

## What ran

The previously live-tested reset, SEC, PI HOB and DXE IPL path embedded three
PE/COFF images in dispatch order: the fixed-rebased generic DXE Core, the
BridgeVM `PlatformTablesDxe` driver, and the BridgeVM marker probe. The
platform driver validated boot-info v1, the complete ACPI table set, the
FADT-to-DSDT link, the SMBIOS 3 entry point and the bounded structure stream.
It then installed the standard ACPI 2.0 and SMBIOS 3.0 configuration-table
GUIDs through DXE Boot Services.

The marker probe ran only after the platform driver because its dependency
expression requires both table GUIDs. It wrote stage `8` and the EFI system
table pointer to guest RAM. The host did not accept the stage marker alone: it
dereferenced the system table inside mapped RAM, required the standard
signature, bounded the configuration-table count to `5..=32`, rejected
duplicate ACPI or SMBIOS entries, and required the published pointers to equal
the board's boot-info RSDP and SMBIOS anchor GPAs. The passing table contained
seven entries and published:

| Interface | Required pointer |
|---|---:|
| ACPI 2.0 configuration table | `0x2600_1000` |
| SMBIOS 3 configuration table | `0x2600_c000` |

The reproducible development FD is 64 MiB with SHA-256
`1227e77889f26cb19c0e2fef2b446b727c39fa652b863c21474692dd65128873`.
Two consecutive builds compared byte-for-byte equal. The pinned inputs and
outputs were:

- EDK2 commit: `b03a21a63e3bd001f52c527e5a57feddb53a690b`
- GCC: `aarch64-elf-gcc (GCC) 16.1.0`
- GNU ld: `GNU ld (GNU Binutils) 2.46.1`
- 6,144-byte reset/SEC/HOB/IPL/exception vector SHA-256:
  `a8d8a79279903253dd7dcc4d34a43aa5c00ac597cf45db613c9d23f03c69ddba`
- fixed-rebased DXE Core SHA-256:
  `b4ca5c00ef7e1b4104776005fe3c07978c78e39d92d2f035cfa72edabdf77d10`
- BridgeVM PlatformTablesDxe SHA-256:
  `16b3fdd6ede6d5aea14d26419351cf262ef358692fd28682dbbafe74c22438b5`
- BridgeVM marker probe SHA-256:
  `463912d8120a00dbcf1cc2493857b318b092889fcd473df53fc1bfa363c4afac`
- combined firmware-volume SHA-256:
  `6b78484a8fca00ad55385a0d32910bc0a138ad2043b4eecf22c5696ccac1b0b1`
- BridgeVmPcPkg source-tree SHA-256:
  `2720653945f6b9b414fed4ceb3c3cdf223d141be6518f7aeab5a4c8e0b48d6f2`
- build-receipt SHA-256:
  `828cbef0af3a6fc2759c94b42604563aede8f42484c4fb9ed543f17895c011e9`

After the new driver was added, the earlier reset/SEC/HOB FD remained
byte-identical at SHA-256
`8db976249ff86c9613d0a13a0d811ee68a94ef90835d524deb451b927bc332d6`,
and its live regression passed again.

## Failed candidate retained

The first platform-table candidate did not dispatch the marker. Its fail-closed
exception return was:

```text
hvc_iss=0x3 args=[96000021, 11ffa938c, 2600228f]
Error: "DXE dispatch stage is 0; expected 8"
```

`ESR_EL1=0x96000021`, `ELR_EL1=0x11ffa938c` and `FAR_EL1=0x2600228f`
identified an alignment fault inside `PlatformTablesDxe`. Disassembly at image
offset `0x138c` showed `ldr w10, [x7,#4]`, a direct 32-bit read of the packed,
not necessarily aligned ACPI header `Length` field. Existing 64-bit XSDT and
FADT link reads already used unaligned helpers, but generic header
`Signature` and `Length` reads did not. The final driver reads both header
fields with `ReadUnaligned32`; the next live candidate produced the passing
receipt above. The failed run remains a failure and is not counted as evidence.

## Claim boundary

This result proves one reset-to-DXE run on Hypervisor.framework in which the
BridgeVM platform driver published the exact ACPI 2.0 and SMBIOS 3 pointers
through the EFI system table. It does **not** prove complete UEFI boot or
runtime services, architectural protocols, variables, GOP, block I/O, a boot
manager, Windows installation or Windows boot.

Device, security and fixed-sample reliability gates for the independent board
remain open. The current product continues to use its existing QEMU
`virt`-compatible guest contract, and A9 remains OPEN.
