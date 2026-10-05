# Userspace GIC deactivation and remote CPU wakeups — 2026-10-05

Integrated source checkpoint `9aaf5083ba1c883915307d8552e2e927bd6e0f82`
repairs a missing remote CPU wake notification in the optional userspace GIC.
The default Apple GIC backend and the retracted timer-recovery policy are
unchanged.

## Reproduced transition

An active shared SPI can remain pending and be routed to another CPU. Clearing
its active state through combined-mode `ICC_EOIR1_EL1` or `ICC_DIR_EL1` makes
that target's IRQ line assert. The existing deactivation changed the line but
returned no remote target bit in `kick_mask`; the caller reported only its
local CPU. The production bridge consumes this mask for remote notification.

Six unchanged deterministic fixtures against baseline
`3692db697255b04f53c911ff8c222055caf16c51` give 3 PASS / 3 FAIL. Fixed-route
cross-CPU DIR and retargeted combined/split deactivation fail their remote-kick
assertions. Priority drop without deactivation, unchanged remote line and
private-interrupt controls pass. The unchanged-line cases include no pending
work, masked work and an already asserted line. Baseline raw SHA-256:
`45e1a71f24af915cffe39daa5e7763807522ed28d9a97be686191466b6856d7a`.

## Narrow repair and paired checks

`crates/bridgevm-hvf/src/userspace_gic/deactivate.rs` captures the current SPI
route and its IRQ-line state before the existing active-state clear. It returns
only that target's changed-line notification bit. The system-register caller
keeps its existing local bit. Private interrupt scope, active-vector cleanup,
priority-drop behavior and timer policy are unchanged.

The six fixture bodies are byte-identical between baseline and repair, SHA-256
`476b36d93d1685852953ee0bfcc8991d4710df40f14e961aaaec917494275cd5`.
The complete userspace-GIC filter passes 26 tests in debug and 26 in release,
including the 20 existing tests. Debug raw SHA-256:
`f592348a1d8bf9862d841814b3818b5c576dfa08a9271605cc32d56ff6e7a349`;
release raw SHA-256:
`2cf092cd359ae988ac75df134f5334cba7ea755d3c36d29584eb4fa569ee4775`.

Focused library/test Clippy, formatting, structural budgets and whitespace
checks pass. The initial formatting failure only concerned test-module order;
it remains preserved, then corrected without changing fixture bodies.
Independent review authenticates the source and paired receipts without
replaying tests. The existing GIC module ceiling decreases from 731 to 713;
the new helper and fixture files are registered at actual sizes 31 and 125.

## Evidence boundary

These are deterministic device-model and static-source results. No VM,
Windows boot, physical timer run or guest workload was executed for this repair.
No live failure is attributed to it. The change restores notification behavior
within the optional backend without a new intentional machine-contract
deviation.

The combined successor still requires complete local project and exact-SHA
GitHub-hosted checks. All 29 criterion states, thresholds, known defects and
product wording remain unchanged. No guest performance improvement, default
backend change, hardware criterion result or release promotion is claimed.
