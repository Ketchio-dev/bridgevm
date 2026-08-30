# BridgeVM Virtual ARM PC: reset enters bounded SEC C (2026-08-30)

## Evidence rank and result

This is a **live single-run** receipt, not a fixed-sample product gate. At exact
BridgeVM code head `2d097a066d6a9868df5a99b8c67ab504e3d9c046`, the
BridgeVM-owned reset entry established a RAM stack and entered its freestanding
SEC C continuation on a real `Mac17,9` host running macOS 26.5:

```text
BridgeVM Virtual ARM PC reset-vector probe: PASS
board=com.ketchio.bridgevm.virtual-arm-pc abi=1
reset_vector=0x0 firmware_size=0x4000000 ram=0x100000000
result=1 result_gpa=0x100001000 boot_info=0x26000000
firmware_sha256=745241de5a20d9240ec31c8000abb6f8ad04544a7ba7b9b4fe8c6f9b012cd890
LIVE PROOF: reset at GPA zero entered BridgeVM SEC C and validated boot-info v1
```

The exact command was:

```sh
BRIDGEVM_HVF_ALLOW_LIVE_BRIDGEVM_PC_RESET_VECTOR=1 \
  tests/integration/bridgevm-pc-reset-vector-live-opt-in-smoke.sh
```

The ad-hoc-signed debug probe carried only the Hypervisor.framework entitlement.
Its post-signing SHA-256 was
`30e58d5a0e63b44ef1af2790a6a7d3ebc279c6751c0e6a118f97fdfb323f3b16`.
The exact-head command output had SHA-256
`4ac7d7e985142a487fd9fac0f0fa44e83af55d07de9156d2798e2c6ecf828b2b`.
Those identities are local probe receipts, not redistributable artifacts.

GitHub-hosted CI run `33333834361` and Security and quality run `33333835884`
both completed successfully at the exact evidence head.

## What ran

The primary reset path masked interrupts, selected the boot CPU, set `SP` to
`0x1_0002_0000`, and called `BridgeVmPcSecMain`. The freestanding C code used
no runtime library and fail-closed validated:

- the exact `BVMBOOT1` magic, ABI, header and image sizes;
- the exact valid flag, every reserved field, and the 112-byte checksum;
- the fixed RSDP, ACPI and SMBIOS addresses and bounded non-empty lengths;
- RAM base and size against the stack and high-MMIO boundaries; and
- a CPU count in the v1 range `1..=64`.

Success returned stage `1`; corrupt magic, header shape, checksum, table range
and machine geometry return distinct stages `2..=6`. The reset assembly rebuilt
the fixed result GPA after the C call, stored the stage, and exited through HVC.

Pinned GCC 16.1.0 and GNU binutils 2.46.1 produced a 616-byte reset/SEC entry
at offset zero of the 64 MiB development flash image. Deterministic checks
included a native C harness that accepted the valid v1 handoff and rejected
bad magic, ABI, reserved data, checksum, table range and CPU count. The firmware
boundary, structural budgets, Rust example tests, clippy and test-reachability
checks also passed.

## Failed experiment retained

The first SEC live worktree candidate did not reach HVC. Its two-second watchdog
stopped the vCPU and the probe reported `unexpected vCPU exit reason 0`.
Disassembly showed that reset assembly had placed the result GPA in caller-saved
register `x1` before calling C, then used `x1` after return. The compiler legally
reused `x1`, so the subsequent store targeted an invalid address.

The correction reconstructs the result GPA only after `BridgeVmPcSecMain`
returns. A source-order guard now rejects a result-address load before the call.
The failed worktree run is not counted as a pass.

## Claim boundary

This result proves that the BridgeVM Virtual ARM PC reset entry can establish a
stack in its own RAM contract, cross the AArch64 C ABI, and execute a bounded
BridgeVM-owned SEC validator on Hypervisor.framework.

It does **not** construct a PI HOB list, locate or load a firmware volume, enter
DXE core, publish a UEFI system table, provide boot/runtime services, variables,
GOP, block I/O or a boot manager, or boot Windows. Device, security and
fixed-sample reliability gates for the independent board remain open. The
shipping Windows engine retains its existing QEMU `virt`-compatible contract.
