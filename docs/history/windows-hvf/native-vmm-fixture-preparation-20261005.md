# Native VMM development fixture preparation — 2026-10-05

The development Windows boot campaign could not reach a desktop because its
existing account had an expired password. Its first failed lane and seven
canceled jobs remain a failed/incomplete eight-job campaign with zero valid
performance samples. This source prepares a separate fresh development fixture
using the existing scripted installer, CSPRNG account writer and production
BootSeed/Secure Boot routines. It does not modify the canonical inputs.

## Ownership and evidence boundaries

Integration `0426f386` adds the D11 preparation tier. Sealed private inputs and
a separate signed runner feed a capacity-limited APFS container. The controller
requires an installed image, real guest READY plus SERVICE, a nonce-bound new
account and desktop reply, then natural shutdown and absent owned writers.
Nested mounts must normally detach before media sealing; uncertain cleanup
fences the worker. Staged executable identity is held through both VM phases.
Generated credentials stay in required private files/media; guest commands and
public receipts contain none. Product account templates remain unchanged.

The fixed container is admitted only with the entire capacity, 2 GiB overhead
and 104 GiB host reserve available. Sparse logical sizes do not establish fit.
Timeout, ENOSPC or failed readiness remains incomplete without automatic retry.
The preparation always reports DEVELOPMENT_ONLY and t15_ready=false. Export,
restaging and a new performance campaign require separate admission and full
stable hashes. Actual installed-worker cleanup hooks must be upgraded and
reviewed before submitting D11; a new job harness does not update that worker.

## Deterministic verification and preserved failures

The author fixture passes 49 Python cases, seven mocked PowerShell 7 cases
and the compiled Swift helper contract; the existing D10 suite passes 63 cases.
Windows PowerShell 5.1 execution is configured on GitHub-hosted Windows and
remains pending at this checkpoint. No test executes a real Windows guest.

A staged-runner mutation regression first failed because altered copied bytes
could still produce a sealed fixture. Holding a FileSeal through phase and
cleanup boundaries repairs it. A separate failed fixture detected an inherited
zero-reboot first-boot setting; preparation now uses the installed wrapper's
existing bounded eight-reboot setup allowance. The PowerShell mock-provider
scope failure was corrected and retained; it is not a guest result.

Root authenticated 91 handoff descriptors and 41 unchanged integrated files;
the sole initial conflict was append-only budget registration. Root review then
found that missing output after an attempted preparation could be classified
as a clean pre-execution refusal. The new paired test produced 1 PASS/2 FAIL.
An immutable queue attempt record now precedes output creation. Missing output
with that record cannot prove absent writers and forces cleanup-unproved.
The same three cases pass after repair. Extracted run/schema modules preserve
all existing structural ceilings; no criterion or capacity limit was relaxed.

## Validation boundary

The integrated source still requires its complete local check, exact pushed-SHA
hosted checks and a reviewed actual-worker upgrade. Real installation fit,
account readiness and a reusable T15 pair are unproven. Current cleanup guards
check backing identity, while archive reads preserve historical proof; neither
substitutes for downstream full content hashing. No Windows performance gain,
product criterion pass or release promotion is claimed.
