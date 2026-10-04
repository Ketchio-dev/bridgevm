# T17 failed pilot and selection-attempt candidate — 2026-10-04

This records one failed development pilot and a proposed source repair.
The [preceding failed pilot](t17-fresh-selection-graph-20261003.md) remains
failed. No capability state, criterion or threshold is promoted.

## Preserved physical result

Source `5da403e75039736fbcfed35a8403aecfd67048ee` was sealed for job
`codex-t17-5da403e7-fresh-selection-pilot-r1`, submitted once at
05:16:29 UTC. The original [public receipt](../../windows-arm/evidence/t17-5da403e7-fresh-selection-pilot-failure-20261004.json)
reports one run, zero passes, one failure, and cleanup verified true.
The worker started at 05:17:42 and finished at 05:25:25 UTC.

Artifact preflight, VM creation, source preparation, Windows installation and
Secure Boot provisioning each have one recorded stage. All ten counts from
first READY through second shutdown remain zero. These counters do not prove
that the guest runtime reached READY or completed the product journey.
The public failure is `product-model-failed`; the lane reports
`input-selection-failed` at `accept-selection`.

The recorded operation nests selection-button lookup through snapshot-attempt
into an AX relationship read at its returned boundary. Selection-ready took
14,442.706 ms with 3,850.110 ms remaining. Acceptance took 3,850.875 ms and
returned with -0.780 ms remaining. These are whole-operation durations;
no individual AX call, successful selection press or underlying cause is
isolated. The 12,000 bound is a distinct-node ceiling. The shared chooser
deadline is 20 seconds; no per-call AX timeout is claimed.

The original public CI/security flags stay false, run identifiers and final
hashes stay absent, and all promotion flags stay false. Separate hosted source
receipts do not rewrite this failed producer record. A9 and A11 remain OPEN;
product state remains ENGINEERING_PREVIEW.

## Two bounded partial captures

Two actual wrapper calls returned zero at the fixed submission+240-second and
+420-second targets. Invocation lateness was 0.0512895 and 0.092678458 seconds.
No retry or clock-origin change was recorded. Both captures remain partial,
complete=false and criterion=false, with host-stamp authentication pending
in their original records.

The first retained 4,673,051 installer-staging bytes and a zero-byte app log;
committed installer and native runtime logs were missing then. The second
retained 7,728,002 staging bytes and 7,728,002 committed installer bytes,
with matching hashes, plus a zero-byte app log; the native runtime log was
still missing. Retained raw-log hashes were rechecked offline. Stable partial
reads do not make these logs complete or prove native-runtime progress.

The two copied genuine samples record unlocked/frontmost state and the same
admitted app identity, with no sampled refusals. This is finite sampled
ownership, not continuous state or independent descendant-quiescence proof.
The owned orchestrator returned zero after both wrappers; its return is not
the physical pilot's result and does not override that failure.

Both retained request copies are 3,264 bytes with the same SHA-256. A later
host stamp binds that exact request and the failed result body for the same
source, job, lane and nonce. It does not retroactively complete either capture.
Failed-lane media hashes are producer reports within the authenticated body,
not independently host-authenticated disk, vars or guest evidence.

## Proposed source repair and bounded developer feedback

Static source shows readiness and acceptance performing separate fresh
selection lookups. The candidate combines owner resolution, enabled read and
an admitted press into one fresh selection attempt. An absent, disabled or
retryable read result reacquires a fresh graph on the next poll. Once a press
is entered, its failure or uncertain return cannot become a polling replay.
These are sequential native calls; the attempt is not atomic and synchronous
AX calls are not advertised as preemptible. The shared 20-second deadline,
12,000 distinct-node ceiling, final chooser dismissal and exact selected-path
proof remain unchanged.

The retained baseline executed one new production-flow regression, failed
with six assertions, and returned 1 after 75.477 seconds. The candidate's
focused native run returned 0 after 8.167 seconds: 177 XCTest cases, zero
failures, including eight selection-attempt cases. Those cover the normal
route, fresh absent/disabled replacement, delayed lookup/enabled reads,
transient reads, and press failure/late-return without replay.

This is automated/static evidence, not proof of live cause or repair.
Source checkpoint `f5e25120d207dd17f45f767d6aaf79e2e0c2bf0f` records the candidate.
Independent source and focused-result reviews are complete. Full local
project checking, exact-source hosted CI and a new live run remain pending.
Private capture scaffolding was agent-authored and executed by root; a
separate offline peer reviewed its partial-return semantics. The public
record contains metadata, hashes and counts; private request/log/media bytes
and machine paths are excluded. Prior failures and open gates remain.
