# BridgeVM Virtual ARM PC: GIC timer IRQ reaches EL1 (2026-08-30)

## Evidence rank and result

This is a **live single-run** receipt, not a fixed-sample product gate. At exact
BridgeVM code head `5db0948751d41f2f8f200e01431f6c7ea5ee355e`, the opt-in
probe passed on a real `Mac17,9` host running macOS 26.5:

```text
BridgeVM Virtual ARM PC IRQ probe: PASS
board=com.ketchio.bridgevm.virtual-arm-pc abi=1
gic_dist=0x20000000 gic_redist=0x21000000 gic_msi=0x23000000 ram=0x100000000
geometry=AppleGicGeometry { distributor_size: 65536, distributor_alignment: 65536, redistributor_region_size: 33554432, redistributor_size: 131072, redistributor_alignment: 65536, msi_region_size: 65536, msi_region_alignment: 65536, spi_intid_base: 32, spi_intid_count: 988 }
flag=1 vtimer_exits=0
LIVE PROOF: BridgeVM Virtual ARM PC v1 GIC delivers the architected-timer PPI
```

The exact command was:

```sh
BRIDGEVM_HVF_ALLOW_LIVE_BRIDGEVM_PC_IRQ=1 \
  tests/integration/bridgevm-pc-gic-irq-live-opt-in-smoke.sh
```

The ad-hoc-signed debug probe carried only the Hypervisor.framework entitlement.
Its post-signing SHA-256 was
`3292bb477ec010a47dbc1236e4f0b34e886dd4180950c0d60e39a2304758f2d8`.
That binary identity is a local probe receipt, not a redistributable artifact.

## What ran

The probe queried Hypervisor.framework's distributor, redistributor, MSI and SPI
geometry and passed it through `plan_for_geometry` before creating the GIC. It
then:

1. mapped a private 64 KiB executable allocation at the board's
   `RAM_BASE=0x1_0000_0000`;
2. created one vCPU with the board's CPU-0 affinity;
3. entered a minimal EL1 guest that configured `ICC_SRE_EL1`, `ICC_PMR_EL1`,
   `ICC_IGRPEN1_EL1`, the distributor and the CPU-0 redistributor SGI frame;
4. enabled PPI 27, armed `CNTV`, and unmasked IRQs;
5. acknowledged and ended the interrupt in the guest handler, which wrote one
   to the mapped flag page.

The AArch64 address-loading instructions are generated from
`machine::bridgevm_pc` constants. A deterministic example test pins the exact
encodings for the v1 GIC and RAM addresses, so the live probe cannot silently
fall back to the current QEMU `virt`-compatible map.

Focused deterministic checks passed before the exact-head live run:

```text
cargo test -p bridgevm-hvf --example bridgevm_pc_irq_live: 1 passed
cargo clippy -p bridgevm-hvf --example bridgevm_pc_irq_live -- -D warnings: PASS
structural budgets: PASS
scripts/check-tests-are-reachable.py: 321 files, 2088 tests, PASS
```

The first candidate head, `fbb052f571dfc2d2c74a3a7ae415ef0b34165aa2`,
produced the same live `flag=1` signal but is not retained as releasable
evidence: hosted CI `33327851796` failed because the new example test was not
registered in the reachability gate, and Rust 1.98's advisory clippy job found
two pre-existing `drain(..).collect()` sites. Head `464978c` registered the
example suite and replaced those drains with `mem::take`, but hosted CI
`33328116155` then correctly caught that this changed the internal IRQ queues'
capacity-preservation contract. Head `6246297` drains by appending to a new
result vector, preserving the source queues' allocation; its focused capacity
test passed. Exact evidence head `5db0948` also removes a redundant test import
identified by Rust 1.98. Its latest-stable workspace clippy, reachability gate,
and exact-head live probe all passed.

## Claim boundary

The result proves one narrow integration boundary: Apple HVF can map guest RAM
at the BridgeVM Virtual ARM PC v1 address, instantiate the planned GICv3 regions,
and deliver an architected virtual-timer PPI in-kernel to an EL1 handler.

It does **not** prove external device SPIs, MSI delivery, multi-vCPU routing,
firmware tables, UEFI services, Windows installation or boot, any device model,
TPM/Secure Boot, or fixed-sample reliability. Those gates remain open, and the
shipping Windows engine retains its QEMU `virt`-compatible guest contract.
