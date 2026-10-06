# Userspace GIC priority writes and CPU wakeups — 2026-10-05

Source checkpoint `f2c88265749f415ac58fe57612ff292ac8887494` repairs a missing
wake notification in the optional userspace GIC backend. The default Apple
in-kernel GIC backend and the retracted timer-recovery policy are unchanged.

## Reproduced state transition

Distributor and redistributor priority MMIO writes updated pending interrupt
deliverability but returned no target CPU kick. The production bridge consumes
`kick_mask` to wake remote vCPUs and refresh interrupt injection, so changing the
IRQ line without reporting that transition left that path unnotified.

The same three deterministic fixtures against old production give 1 PASS and
2 FAIL: distributor and redistributor priority changes omit their kicks. The
formatted baseline raw SHA-256 is
`d6eb5778096ec2008197b52b639c2df0b7676120afafe9e2ccc67c4cdc809379`.
The initial unformatted fixture and a handoff-reader failure remain recorded;
neither is rewritten into a passing result.

## Narrow repair and coverage

`crates/bridgevm-hvf/src/userspace_gic/priority_mmio.rs` contains the extracted
priority-byte packing, width and span logic. Accepted distributor writes compare
IRQ-line state for each configured CPU; packed writes can affect different CPU
routes. Accepted redistributor writes compare only the addressed CPU. Both use
the existing changed-line helper to add only the relevant bits to `kick_mask`.
Reads, repeated writes and rejected spans do not kick. Active or still-masked
interrupts add no kick when IRQ-line state is unchanged; other state is preserved.

`crates/bridgevm-hvf/src/userspace_gic/priority_update_tests.rs` covers packed
multi-CPU promotion and masking, remote private interrupts, reads, repeated
writes, active interrupts and invalid spans. The repaired userspace-GIC suite
passes 20 tests in debug and 20 in release with the same final fixture bytes.
Debug raw SHA-256 is
`319299bf70fc97a808b89703a695838bdad08c6a03d0d6049ec62bd338000b6b`;
release raw SHA-256 is
`9e2cc2eded45559e8e29e6bd88ecdf449dc5f5c3f0995d32084950b72b7ff804`.
Formatting, Clippy, structural budgets and whitespace checks pass. Independent
source review covers routing, addressed-CPU scope and no-kick controls; it does
not independently replay the tests. Existing MMIO size ceiling decreases from
352 to 310; new modules are registered at their actual counted sizes, 86 and 82.

## Evidence boundary

These are deterministic test and static-source results. No VM, Windows boot,
physical timer run or guest workload was executed for this repair, and no live
failure is attributed to it. This restores expected userspace-GIC notification
behavior without a new intentional machine-contract deviation.

The combined successor still requires its own complete local project check and
exact-SHA GitHub-hosted checks. Prior failed experiments and dated pending
checkpoints remain unchanged. A9, A11 and A19 remain OPEN; product state,
criterion thresholds, known defects and release flags are unchanged. No guest
performance improvement, default-backend change or release evidence is claimed.
