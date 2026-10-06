# Userspace GIC pending-state readback — 2026-10-05

Integrated source checkpoint `633404c9a8f17edfcdba7429f55471229b818bda`
repairs guest pending-register reads in the optional userspace GIC. An asserted
level-sensitive SPI could raise the target CPU's IRQ line while both
`GICD_ISPENDR` and `GICD_ICPENDR` incorrectly reported it as not pending.

## Reproduced state and architectural contract

The distributor stores software/edge pending state separately from device line
levels. Interrupt delivery already combined them; the guest pending-register
read path returned only the latch. Acknowledging a still-asserted level SPI
also exposed the mismatch: it was active and pending, with delivery suppressed
while active, but both pending aliases read zero.

[Arm IHI 0069F](https://documentation-service.arm.com/static/6012f0024ccc190e5e68124f)
section 4.1.2, printed page 4-52, defines that level-sensitive acknowledgement
transition. Sections 11.9.11 and 11.9.26, printed pages 11-526 and 11-556,
require both aliases to report pending and active-and-pending state. Clearing
the software latch does not suppress a still-asserted level input.

Six unchanged deterministic fixtures on the old implementation give
2 PASS / 4 FAIL. Asserted-level, active-and-pending, disabled-input and
clear-pending-with-level-high cases fail their readback assertions. Software
pending after input deassertion and edge pending through acknowledgement pass.
The fixtures cover implemented SPI bank boundaries and a CPU1 route. Baseline
raw SHA-256: `3c5995f871e0c83e4efca3cbe81b7197402848bd860791314d017917e8a244ce`.

## Repair and paired checks

`crates/bridgevm-hvf/src/userspace_gic/mmio_regs.rs` uses the existing effective
pending state for the two read aliases after its aligned, in-range access
check. Register writes, notification masks and interrupt-state transitions are
unchanged. Existing register-field helpers move byte-for-byte into
`crates/bridgevm-hvf/src/userspace_gic/register_fields.rs` to preserve budgets.
The MMIO file ceiling decreases from 310 to 283; the new helper and fixture
ceilings are their actual sizes, 35 and 125.

Fixture SHA-256 is identical before and after the repair:
`466398e09b8268802148aad92bbd8e652ec6885c0ebeac426caa5be7e36e1bf1`.
The complete userspace-GIC filter passes 32 tests in debug and 32 in release,
including all six new fixtures and 26 existing tests. Debug raw SHA-256:
`cbf9f9660448799b0e493e92db92b70da3c488f1a335d9efa860b139a5f384a6`;
release raw SHA-256:
`db22ef245bc5ee59a6b28e9104190545e89f810abedd6f243668808b43d92e26`.
Focused library/test Clippy, formatting, structural budgets and whitespace
checks pass. Independent source and retained-evidence review found no blocker.

## Evidence boundary

These results establish deterministic register behavior only. No VM, Windows
boot, physical timer run or guest workload was executed for this repair, and
no live failure is attributed to it. It restores the architectural readback
contract without introducing an intentional machine-contract deviation.
The combined successor still requires complete local project and exact-SHA
GitHub-hosted checks. Criterion states, thresholds, known defects and product
wording are unchanged; no hardware criterion or release promotion is claimed.
