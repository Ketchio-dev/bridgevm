# BridgeVM Virtual ARM PC: EL1 reads boot-info v1 (2026-08-30)

## Evidence rank and result

This is a **live single-run** receipt, not a fixed-sample product gate. At exact
BridgeVM code head `d7a9d191b8acd37939a94e5ef2f14cceb83d8d13`, a bounded
EL1 guest passed on a real Hypervisor.framework host:

```text
BridgeVM Virtual ARM PC boot-info probe: PASS
board=com.ketchio.bridgevm.virtual-arm-pc abi=1
boot_info=0x26000000 size=0x10000 ram=0x100000000
guest_result=1 header_checksum=0 rsdp=0x26001000 xsdt=0x26002000
LIVE PROOF: EL1 read BridgeVM boot-info v1 and followed its RSDP to XSDT
```

The exact command was:

```sh
BRIDGEVM_HVF_ALLOW_LIVE_BRIDGEVM_PC_BOOT_INFO=1 \
  tests/integration/bridgevm-pc-boot-info-live-opt-in-smoke.sh
```

The ad-hoc-signed debug probe carried only the Hypervisor.framework entitlement.
Its post-signing SHA-256 was
`986c95d958cab5d3bded0d8f4529475d464783eda21cf0a0d371a89a8f94ac2b`.
That binary identity is a local probe receipt, not a redistributable artifact.

GitHub-hosted CI run `33330720006` and Security and quality run `33330720930`
both completed successfully at the exact evidence head.

## What ran

The host built the one-CPU, 512 MiB BridgeVM Virtual ARM PC table bundle and
copied its complete 64 KiB boot-info image into aligned private memory. The
probe then:

1. mapped that image read-only at the contract GPA `0x2600_0000`;
2. mapped a separate 64 KiB executable/result allocation at
   `RAM_BASE=0x1_0000_0000`;
3. entered a minimal EL1 guest with the boot-info and result GPAs in registers;
4. checked the `BVMBOOT1` magic, ABI v1 and the 112-byte header checksum;
5. followed the header's RSDP pointer, checked `RSD PTR `, required the RSDP
   XSDT pointer to equal the header ACPI pointer, and checked `XSDT`;
6. wrote stage result `1` to guest RAM and exited through HVC.

The guest uses stage-specific results `2..=7` for each failed condition. A
two-second host watchdog exits the vCPU if the bounded guest does not return.
The boot-info aperture is mapped without guest write permission.

Focused deterministic checks passed before the exact-head run:

```text
cargo test -p bridgevm-hvf --example bridgevm_pc_boot_info_live: 2 passed
cargo clippy -p bridgevm-hvf --example bridgevm_pc_boot_info_live -- -D warnings: PASS
scripts/check-tests-are-reachable.py: 324 files, 2104 tests, PASS
structural budgets: PASS
```

## Claim boundary

The result proves one narrow integration boundary: Apple HVF can map the
BridgeVM Virtual ARM PC v1 boot-info aperture at its contract GPA, and an EL1
guest can read the versioned header and follow its RSDP to the finalized XSDT.

It does **not** execute a SEC, PEI or DXE phase; publish a UEFI system table;
provide variables, GOP or block I/O; boot an installer or Windows; exercise a
device interrupt; or establish fixed-sample reliability. Independent firmware
and every Windows/device gate remain open. The shipping Windows engine retains
its QEMU `virt`-compatible guest contract.
