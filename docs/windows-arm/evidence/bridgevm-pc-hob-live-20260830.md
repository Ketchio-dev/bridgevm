# BridgeVM Virtual ARM PC: SEC constructs a bounded PI HOB list (2026-08-30)

## Evidence rank and result

This is a **live single-run** receipt, not a fixed-sample product gate. At exact
BridgeVM code head `10a0aae10b22e2aa88780fcac17088d7acdd83e8`, the
BridgeVM-owned SEC continuation validated boot-info v1 and constructed its
bounded PI HOB list on a real `Mac17,9` host running macOS 26.5:

```text
BridgeVM Virtual ARM PC reset-vector probe: PASS
board=com.ketchio.bridgevm.virtual-arm-pc abi=1
reset_vector=0x0 firmware_size=0x4000000 ram=0x100000000
result=1 hob_count=5 hob_list_gpa=0x100004000 hob_list_size=176 result_gpa=0x100001000 boot_info=0x26000000
firmware_sha256=8db976249ff86c9613d0a13a0d811ee68a94ef90835d524deb451b927bc332d6
LIVE PROOF: BridgeVM SEC validated boot-info v1 and built the bounded PI HOB list
```

The exact command was:

```sh
BRIDGEVM_HVF_ALLOW_LIVE_BRIDGEVM_PC_RESET_VECTOR=1 \
  tests/integration/bridgevm-pc-reset-vector-live-opt-in-smoke.sh
```

The host had booted at 2026-08-30 11:34:29 EDT; this exact-head run occurred
after that boot. The ad-hoc-signed debug probe carried only the
Hypervisor.framework entitlement. Its post-signing SHA-256 was
`97bb5d3de2601632121b668b5e7cd53f0a580f5488b4d8880abe10fb8f5cc836`.
The captured command output had SHA-256
`fb9cd902d7ab43ab2120cd80ee5531bea95ab92e3e342b984126eadded4e8f23`.
Those identities are local probe receipts, not redistributable artifacts.

GitHub-hosted CI run `33334711841` and Security and quality run `33334713040`
both completed successfully at the exact evidence head.

## What ran

The primary reset path passed fixed pointers for boot-info, the SEC result and
the HOB buffer in `x0..x2`, established `SP=0x1_0002_0000`, and called the
freestanding `BridgeVmPcSecMain`. SEC first fail-closed validated the complete
boot-info v1 handoff. It then wrote a 24-byte result at `0x1_0000_1000` and a
176-byte, five-entry PI HOB list at `0x1_0000_4000`:

| Offset | HOB | Length | Proven contents |
|---:|---|---:|---|
| `0` | PHIT | 56 | PI version 9, full-configuration boot, RAM and free-memory bounds, end pointer |
| `56` | system-memory resource | 48 | RAM base `0x1_0000_0000`, 512 MiB length, present/initialized/tested/write-back attributes |
| `104` | stack allocation | 48 | standard stack GUID, base `0x1_0001_0000`, length 64 KiB, BootServicesData |
| `152` | CPU | 16 | 40 physical-address bits and zero I/O-address bits |
| `168` | end | 8 | standard end-of-list type and zero reserved field |

The PHIT marks `0x1_0003_0000` as the first free byte, keeping the SEC result,
HOB list and stack below free memory. Compile-time assertions fix every
structure size, the total list size, non-overlap and the assembly stack top.
The boot-info validator rejects RAM too small to contain that reserved area.

The live host parser did not accept the result stage alone. It independently
decoded every field listed above from guest RAM after the HVC exit and failed
closed on a wrong stage, count, GPA, size, header, reserved field, GUID, memory
bound, resource attribute, stack property or CPU width.

Pinned GCC 16.1.0 and GNU binutils 2.46.1 produced a 1,168-byte reset/SEC/HOB
entry with SHA-256
`0d01124e1c7619504eea1b7fd2ac6149ec415e4e494d753becc5bac0d01837ff`.
The complete 64 MiB development FD has the firmware SHA-256 shown above. The
input source tree recorded in its build receipt has SHA-256
`dd433b2b857b0622ee30fb13fc205b3bed4bc20ddbdaf71614b4a7ce04b69875`.

Deterministic checks covered valid construction, corrupt boot-info classes,
insufficient RAM, null HOB input, the exact HOB sequence and a corrupt CPU HOB.
The firmware boundary, reproducible build, Rust example tests, Clippy,
structural budgets and test reachability also passed; the exact-head
reachability check reported 327 files and 2,109 tests.

## Failed deterministic checks retained

The first native HOB test harness used a 32-bit comparison helper for GPAs above
4 GiB. `-Werror` rejected the narrowing constants before execution. After the
helper was corrected to compare 64-bit values, the end-HOB check exposed a
second harness-only signed-promotion error for type `0xffff`; an explicit
unsigned cast corrected that expectation. Neither failed deterministic run is
counted as live evidence. The exact-head live candidate itself passed.

## Claim boundary

This result proves one reset-to-SEC run constructed and exposed the exact
bounded PI HOB list described above on Hypervisor.framework. It advances the
independent board beyond boot-info validation, but it is not a complete UEFI
firmware result.

The image still contains no firmware volume, PEI, DXE core, UEFI system table,
boot or runtime services, variables, GOP, block I/O, boot manager or Windows
loader. Device, security and fixed-sample reliability gates for the independent
board remain open. The shipping Windows engine retains its existing QEMU
`virt`-compatible contract.
