# Authenticate archived A19 quota/interruption reads — 2026-10-09

Evidence rank: deterministic synthetic CLI tests and static review. No live
Windows, current release campaign, quota workload or power-loss proof.

The actual `bridgevm-live receipt` route handled T21/T22 as generic historical
byte streams. A synthetic done-queue archive built from public historical
receipts had valid private data and matching job/readonly ledger seals. Changing
only the public app-artifact hash to a different schema-valid SHA-256 was still
returned successfully for both tiers. Baseline: two tests, two failing subcases.

The strict archive reader now reuses bounded no-follow stable reads and existing
tier validators, compares public/private values, authenticates the exact queue
and ledger seal, requires done placement and refuses tier-owned media residue
(including dangling aliases) before emitting anything. T20/T23 keep their
existing strict readers; T1 and D10/D11 routing stay separate. Generic unrelated
historical compatibility remains. No retained receipt is rewritten.

Review also identified a malformed public-only tier hint lost by strict JSON
parsing when job/ledger identities were generic. Actual CLI reproduction: one
test, six failing subcases across both tiers and duplicate-key orderings/NaN.
A separate bounded routing-only parser retains any T21/T22 hint, including
nested hints; duplicate/nonfinite data still fails strict acceptance. This does
not claim protection against a writer able to replace all archive evidence.

Focused twelve tests cover valid schema-1, partial and three-case schema-2,
authenticated failed-clean receipts, changed/missing/malformed originals,
unsafe/oversize files, queue seals, nonterminal placement, residue, downgrade
hints and matching invalid public/private promotion flags. Removing the
correspondence check reproduces both changed-hash failures; mutation removed.
The driver test now injects failure into every A19 suite to verify fail-fast
execution. Restored A19/archive/routing checks passed 34 suites/219 tests. No
assertion, deadline, sample count or ceiling relaxed; exact full/hosted pending.

Archive validity does not establish release acceptance. Historical schema-1
and partial schema-2 remain valid at their recorded scope; release review still
requires current-head T21, schema-2 T22 with all three interruption cases, and
one fixed successful T23 campaign of ten lanes/thirty natural boots. All
nonpromotion fields and A9/A11/A19 OPEN states remain unchanged.
