# BridgeVM Virtual ARM PC: reset vector executes at GPA zero (2026-08-30)

## Evidence rank and result

This is a **live single-run** receipt, not a fixed-sample product gate. At exact
BridgeVM code head `5ed86f6bfa9bfb615bd2308178002710aa34f958`, a bounded
development reset image passed on a real `Mac17,9` host running macOS 26.5:

```text
BridgeVM Virtual ARM PC reset-vector probe: PASS
board=com.ketchio.bridgevm.virtual-arm-pc abi=1
reset_vector=0x0 firmware_size=0x4000000 ram=0x100000000
result=1 result_gpa=0x100001000 boot_info=0x26000000
firmware_sha256=af815a96240bb3cfd2ab19f6c853b70f609bdfca78f4a0885a08fb3ff9dbdf41
LIVE PROOF: BridgeVM Virtual ARM PC v1 reset vector executed at GPA zero
```

The exact command was:

```sh
BRIDGEVM_HVF_ALLOW_LIVE_BRIDGEVM_PC_RESET_VECTOR=1 \
  tests/integration/bridgevm-pc-reset-vector-live-opt-in-smoke.sh
```

The host had booted at 2026-08-30 11:34:29 EDT; this exact-head run occurred
after that boot. The ad-hoc-signed debug probe carried only the
Hypervisor.framework entitlement. Its post-signing SHA-256 was
`a3aa5f8a058ce132939dc3525b2f5170fda59936132845a2c632c85ee92457f9`.
The captured command output had SHA-256
`060b61f80f95cc19e084afb76bd920401ce81a195876aef9863d7aef8fb0731e`.
Those identities are local probe receipts, not redistributable artifacts.

GitHub-hosted CI run `33333079114` and Security and quality run `33333080459`
both completed successfully at the exact evidence head.

## What ran

The pinned build used `aarch64-elf-gcc` 16.1.0 and GNU binutils 2.46.1 to
produce a 92-byte BridgeVM-owned AArch64 reset entry. The build placed it at
offset zero of an otherwise erased 64 MiB development flash-code image and
verified the complete image against the fixed SHA-256 shown above.

The live probe then:

1. generated the BridgeVM boot-info v1 table bundle and mapped its complete
   64 KiB aperture read-only at `0x2600_0000`;
2. mapped the development flash image read/execute at GPA zero;
3. mapped a separate result page in system RAM at `0x1_0000_0000`;
4. entered a primary AArch64 vCPU at PC zero and EL1h with interrupts masked;
5. observed the reset entry validate the `BVMBOOT1` magic, store result `1` at
   `0x1_0000_1000`, and exit through HVC;
6. bounded the run with a two-second watchdog and unmapped every guest region.

Secondary CPU affinities take a separate permanent park loop. The probe tests
only the primary path; no multi-vCPU firmware claim follows from this run.

Focused deterministic checks at this tranche passed:

```text
cargo +1.97.0 test -p bridgevm-hvf --example bridgevm_pc_reset_vector_live --locked: 3 passed
cargo +1.97.0 clippy -p bridgevm-hvf --example bridgevm_pc_reset_vector_live --locked -- -D warnings: PASS
scripts/check-bridgevm-pc-firmware-boundary.sh: PASS
scripts/check-refactor-budgets.sh: PASS
scripts/check-tests-are-reachable.py: 326 files, 2107 tests, PASS
```

The complete local `scripts/check-project.sh` also ran. Every executable test
stage passed, but the aggregate correctly returned failure because the
capability registry detected code newer than A11's `tested_commit`. A11 remains
OPEN until the final code head is resealed; that expected freshness failure is
not rewritten as a project-check pass.

## Failed experiment retained

The first live candidate returned `reset vector returned stage 2`. The reset
entry had loaded the eight-byte boot magic into `x2`, then used `w2` for the
result before comparing `x2` with the expected magic. Writing `w2` zero-extended
and destroyed the value that the comparison needed. The corrected entry keeps
the magic in `x2` and uses `w5` exclusively for results. A deterministic source
guard now requires that load/compare/result sequence and rejects a future
`mov w2` reintroduction. The failed run is not counted as a pass.

## Claim boundary

The result proves one narrow integration boundary: Hypervisor.framework began
executing BridgeVM-owned AArch64 code at the BridgeVM Virtual ARM PC v1 reset
GPA, and that code consumed the board's private boot-info handoff from its
fixed address.

The image has no SEC C environment, PI HOB list, PEI, DXE core, UEFI boot or
runtime services, variables, GOP, block I/O, boot manager or Windows loader. It
does not prove device discovery, firmware reset, multi-vCPU startup, Windows
installation, security or fixed-sample reliability. The independent board
remains experimental, and the shipping Windows engine retains its existing
QEMU `virt`-compatible contract.
