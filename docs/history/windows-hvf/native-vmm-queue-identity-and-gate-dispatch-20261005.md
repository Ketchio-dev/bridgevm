# Queue identity and deterministic gate dispatch — 2026-10-05

## Preserve live NVMe queues on invalid creation

Three guest-reachable CREATE defects accompany the separately repaired DELETE
behavior. Recreating a live full CQ replaced its ring and let a blocked write
run without consuming the existing completion. Recreating a live SQ replaced
its ring and cleared its pending command. An I/O SQ could also target adminCQ0.

Three guards now refuse those identities before installing a queue or clearing
pending work. The original creation methods are extracted without changing
size, alignment, maximum-ID checks or existing error precedence. Deletion and
valid recreation remain supported. This implements the duplicate-ID/CQ0 rules
in [NVMe1.4 sections5.3–5.4](https://nvmexpress.org/wp-content/uploads/NVM-Express-1_4-2019.06.10-Ratified.pdf);
no new intentional machine-contract deviation or general conformance is claimed.

R1 remains recorded. R2 makes initialization explicit: 64-byte SQEs, 16-byte
CQEs, negotiated I/O queues and page-aligned mapped bases. The unchanged R2
fixture against old production gives84PASS/3FAIL/1existingIGNORE using the
nvme::tests:: filter; raw SHA
`ae98db0fae2a53c86cd6223fc3d51f7f57416c0f05c9970dc08febb64b97aaec`.
After repair, the wider nvme:: filter gives97PASS/1existingIGNORE in both debug
and release. These filter counts are not identical comparisons. The three
paired cases verify queue state, unread completions and disk bytes; ordinary
completion consumption subsequently resumes the preserved pending write.
Formatting, Clippy, budgets and paired-fixture comparison pass. No live guest
or real disk was used; further source review and integrated checks are required.

## Correct tests after strict T1 routing

The T23 dispatcher inventory expected its literal tier name in the thin wrapper
even though the wrapper delegates non-T1 tiers to the legacy implementation.
The corrected inventory names that implementation, and a new subprocess test
executes the actual wrapper to verify exact spaced arguments and failure47.
Five existing dispatch cases are extracted; real publication, refusal, sealing,
cleanup and no-downgrade checks remain in their entrypoint.

Independent review rejected the first extraction because joining its two suites
with && hid the first failure under Bash set-e. An owned driver regression
reproduced2PASS/1FAIL: an injected receipt failure23 incorrectly returned0.
Sequential commands repair that path without increasing the16-line ceiling.
Both receipt/dispatch failures now stop immediately; the positive trace reaches
every suite and the T22 tail. The rejected candidate and baseline remain.

The first full focused entrypoint then exposed a downstream T22 fixture that
used a bare T1 document as unrelated historical data. It now uses a non-T1
tier while retaining exact returned bytes and unused-cache checks. Full focused
A19/T22 execution passes28 unittest groups/188 cases,28OK/0FAILED in196.385s;
raw SHA `285586a865d0b56dcfb43ef9447282a3ee695d338e1cdcab4121fdd05c269d5b`.
All522 shell scripts, budgets and whitespace checks also pass. Validators,
timer defaults, receipt thresholds and release criteria remain unchanged.

## Evidence boundary

Earlier F59/fae/6d complete-check failures remain failed. These focused results
do not replace this combined successor's full project and exact-SHA hosted CI.
No Windows installation, physical timer/storage result, performance gain,
criterion completion or release evidence is claimed.
