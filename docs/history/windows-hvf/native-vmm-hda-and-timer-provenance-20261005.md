# Native HDA command DMA and timer evidence — 2026-10-05

This checkpoint repairs two command-ring address calculations in the own VMM
and binds the legacy timer experiment to its actual executed artifact. These
are distinct conclusions. The timer algorithm, gate thresholds and capability
states do not change; neither conclusion establishes a Windows performance or
audio-quality improvement.

## HDA command-ring overflow

Real guest MMIO can rewrite the running CORB and RIRB bases. After ordinary
traffic reaches CORB index31 or RIRB index15, the maximum128-byte-aligned base
plus the next entry offset overflows. Corrected paired tests on the old source
produce1PASS/2FAIL in both debug and release: both profiles panic on addition,
because this repository enables release overflow checks. No unchecked-build
wrap execution is claimed. Ordinary traffic remains a positive control.

The two command-DMA methods are extracted unchanged except for checked address
addition and guarded memory calls. Overflow follows the existing CORB memory
error or RIRB overrun path. The tests assert pointers, response state and guest
memory preservation. Fixed debug HDA tests pass21 cases; fixed release boundary
tests pass3 cases. Focused formatting, Clippy and structural checks pass.
Independent review verifies the inverse extraction and both semantic changes.
Author911ddc08 integrates as c47aaa11 with four byte-identical Rust files.

Two earlier findings are explicitly excluded: the initial fixture failed all
three cases before the candidate access because legitimate RINTFL was uncleared;
it was corrected through guest W1C before reproducing the overflow. The proposed
playback BDL base rewrite was not a defect: existing !RUN register guards reject
that route, and restart resets the index. Both records remain retained.

## Legacy timer artifact and receipt binding

T1 still uses the historical recovery policy removed from production on
2026-09-07. Its10000 wakes, cancellation interval0, arm1000 and stall5000 defaults
and native counter verdict are unchanged. It cannot prove current production
timer policy or Windows reliability; the dated scope correction remains in
[the original experiment](../../windows-arm/evidence/t1-vtimer-cancel-microprobe-20260804.md).

The old target path and generic receipt fail two owned regressions. Fresh
builds now select the actual Cargo compiler-artifact executable, sign and
strictly verify it, and bind source/tree, binary, settings, detail and raw hashes.
Cached runs require the matching prior seal. All promotion flags remain false.
Copied T1 data cannot bypass a conflicting queue tier, and booleans cannot
substitute for unsigned counters. Old receipts retain their historical limits.

Independent review found and repaired stale cross-target artifact selection
and conflicting T20/T23 tier routing. Root then observed that actual macOS
codesign emits a text dictionary by default, invalid for plist parsing. The
paired realistic fixture fails before explicit --xml and passes after it.
Read-only extraction from an owned signed executable also parses the true
hypervisor entitlement; the executable was not run in that check.

Final15 owned contract cases pass. Earlier27 native unit cases and the real
Cargo artifact build remain evidence at unchanged native source. Root reviewed
130 source/evidence descriptors and the narrow R2 delta before integrating
5d755829 as6bf2df5e. Fifteen author files remain exact; integration retains D11
routing, adds D10/D11 conflict assertions and unions budget registrations.
The integrated15 T1 and52 D11 Python cases,7 mocked PowerShell cases and Swift
helper check pass. Intermediate fixture and reader failures remain recorded.

## Validation boundary

Preceding exact0e secondary-stop local/full/manualCI/PR checks are green.
Preceding exact99 fixture source passes44 local project stages and its hosted
Windows script workflow; its remaining hosted checks are still pending here.
This new integration requires its own full project check and exact pushed-SHA
hosted checks. No physical T1 run, actual audio repair or criterion promotion
is recorded. Existing structural ceilings decrease or stay fixed.
