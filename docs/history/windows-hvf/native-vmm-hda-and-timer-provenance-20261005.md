# Native HDA command DMA and timer evidence — 2026-10-05

This checkpoint repairs two own-VMM command-ring address calculations, binds
the legacy timer experiment to its executed artifact, and corrects a D11
exception handler. Timer algorithms, thresholds and capability states stay
unchanged. No Windows performance or audio-quality improvement is established.

## HDA command-ring overflow

Guest MMIO can rewrite running CORB/RIRB bases. After normal traffic reaches
index31/15, the maximum128-byte-aligned base plus the next offset overflows.
Corrected paired tests on old source produce1PASS/2FAIL in debug and release;
both panic because repository release overflow checks are enabled. No unchecked
wrap execution is claimed. Ordinary traffic remains a positive control.

The extracted methods change only two checked address additions and guarded
memory calls. Overflow follows existing CORB memory-error/RIRB-overrun handling.
Pointer, response and guest-memory assertions pass: debug HDA21 cases, release
boundary3 cases, plus focused formatting/Clippy/budgets. Independent review
verifies inverse extraction. Author911ddc08 integrates as c47aaa11 with four
byte-identical Rust files and reduced existing structural ceiling.

The initial fixture's3 failures occurred before the candidate access because
legitimate RINTFL was uncleared; guest W1C corrected the fixture before the
paired reproduction. The proposed playback BDL rewrite was not a defect:
existing !RUN guards reject it and restart resets the index. Both records stay.

## Legacy timer artifact and receipt binding

T1 retains the historical recovery removed from production on2026-09-07.
Its10000 wakes/interval0/arm1000/stall5000 defaults and counter verdict stay.
It cannot prove production timer policy or Windows reliability; see the dated
correction in [the original experiment](../../windows-arm/evidence/t1-vtimer-cancel-microprobe-20260804.md).

Old target-path/generic-receipt code fails two owned regressions. Fresh builds
now use the actual Cargo compiler-artifact executable and strict signing checks.
Receipts bind source/tree, binary, settings, detail and raw hashes; cached runs
need a matching prior seal. Promotion flags remain false. Copied T1 data cannot
bypass foreign queue tiers; booleans cannot replace unsigned counters.

Independent review repaired stale cross-target selection and T20/T23 routing.
Root then found actual codesign defaults to a text dictionary, invalid for
plist parsing. A realistic paired fixture fails before explicit --xml and
passes after it. Read-only extraction from an owned signed executable also
proves the true entitlement; that check did not execute the binary.

Final15 owned cases pass. Earlier27 native units and real Cargo build remain
valid at unchanged native source. Root authenticated130 descriptors and the
narrow R2 delta before integrating5d755829 as6bf2df5e. Fifteen author files stay
exact; D11 routing and D10/D11 conflict assertions survive integration. Prior
fixture/reader failures and historical receipts keep their limits.

## D11 exception-handler correction

Root's earlier run-module extraction omitted the subprocess import. Six new
subcases across output creation/execution and three expected exception types
reproduce NameError. The one-import repair preserves immutable attempts and
writes refusal/cleanup-unproved receipts without private exception text. The
same six subcases pass; integrated D11 has53 Python cases,7 mocked PowerShell
cases and a Swift helper check. The99 checkpoint remains unadmitted for live
preparation; its passing tests did not cover this exception path.

## Validation boundary

Preceding0e local/full/manualCI/PR checks are green. Preceding99 local44 stages
and hosted Windows scripts pass; remaining hosted checks are pending here.
This integration needs its own full project and exact pushed-SHA hosted checks.
No physical timer run, audio-quality repair or criterion promotion is claimed.
