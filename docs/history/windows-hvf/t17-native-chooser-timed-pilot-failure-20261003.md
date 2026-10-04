# T17 native chooser timing and failed pilot — 2026-10-03

This is one failed development pilot, not an A9 or release pass.
The [earlier failed pilot](t17-runtime-chooser-pilot-failure-20261003.md) and
[failed required checks](t17-diagnostic-timing-required-check-failure-20261003.md)
remain failed. The shared deadline did not close the live chooser defect.

## Exact source and deterministic admission

Source `9ea516d33becf5a52aaaf280feca1ea260534f31` and tree object
prefix `740fee471ea6` remained frozen (full identity retained privately).
The complete local project check exited zero after 538.866 seconds: 44 outer
headers and 44 PASS. All 4,057 tracked source hashes stayed unchanged.
Hosted standard 37133011187 passed thirteen required jobs; the advisory skipped.
Hosted full 37133060803/job111231879694 checked the exact source and passed
all 43 applicable outer headers. Five companion runs passed eleven jobs.
Each T20 companion recorded seventeen suites and 114 tests; overlapping
selections are not summed. Optional CGL actually failed with 10002/exit101
inside a successful required job; that successful job is not a graphics pass.

An actual Apple Development-signed artifact passed strict signature, packaged
resource, manifest and ten-input verification. It is not a release artifact.
LaunchServices preflight and guarded worker update returned zero. These
preparatory results do not override the failed Windows journey below.

## One actual failed attempt

Job `codex-t17-9ea516d3-shared-chooser-pilot-r1` was submitted once at
16:40:38 UTC; the worker started at 16:40:53 and finished at 16:49:01 UTC.
The producer reports valid/failed, one attempt, zero passes and one failure.
Public failure is product-model-failed; the lane reports input-selection-failed.
Artifact preflight, VM creation, source preparation, Windows installation and
Secure Boot provisioning have one recorded stage each. All ten counts from
first READY through second shutdown remain zero. All promotion flags are false.

The host stamp binds the retained result body and records the request hash.
Original request bytes were not retained, so that hash cannot be rechecked.
Failed lanes skip host artifact authentication. Their private final hashes are
producer reports within a body-bound stamp, not host-verified media evidence.
Public final fields remain absent through successful-lanes-only aggregation.
The original public CI/security flags remain false; separate hosted receipts
are not inserted into this producer record.

## Recorded timing and source limitation

The lane reports selection-ready elapsed 16,034.859ms, remaining 2,616.946ms.
The following accept-selection call took 16,256.910ms and returned with
remaining -13,639.979ms. Selection-confirmation then refused at stage entry.
The whole-call timing does not separate native lookup, enabled reads and
AXPress or identify an underlying AX cause. Failure diagnostics can add time.
Runtime integration selects the host share before starting the guest runtime;
this host chooser failure does not establish a guest stall.

Static reading shows the native driver does not retain the shared deadline.
Panel/button graph walks, transient retries and attribute reads are count
bounded. They can consume the entry allowance before enabled-check/AXPress,
without another deadline check immediately before input. Wait also admits a
predicate read after a pause before checking time. A separate source repair
and delayed-read regression are required. Synchronous AX calls remain
nonpreemptive; no strict whole-call wall-time bound is claimed.

## Retained observation and incomplete capture

The genuine observer exited zero with 26 samples, 16:36:59–16:49:29 UTC,
reason terminal and no sampled refusals. Samples record the same app identity;
continuous state and criterion pass remain false. The app was not reopened.
Exactly one partial capture began at 16:45:59.901564 UTC, 321.274411 seconds
after submission. The planned 3–5-minute target was missed by 21.274 seconds.
The second capture window was missed; no second capture or terminal retry ran.
Installer staging raw: 7,706,711 bytes; app log zero bytes; committed installer
and native runtime logs absent at that capture. Partial means complete=false,
producer terminal unknown and criterion=false, even with a stable read.
Producer reports cleanup; exact work-directory absence was recorded later.
No completed T19 handoff or ready T22 input was produced.

- [Original public receipt](../../windows-arm/evidence/t17-9ea516d3-shared-chooser-pilot-failure-20261003.json): `1d016d85e4f04560d10c6eef1512d18ff5a7cccfcad354f49e2a65959f5c992b`.
- Retained staging raw SHA: `9821a03e6df4fc71cce78650d61c805fca08c66e75a66904828312262c021ae4`.
- Body-bound lane SHA: `a5362995e31bfb993dd564a40756a15e985e6f67573dbef986f27159f94caee2`.
- Host stamp SHA: `78f17f363c5aaa3addd21227d54f48045ce1614bdf6b3ceb5778426e424cb9a3`.

Private media, request/log bytes and machine paths are not copied here.
Capability states, sample thresholds, product wording and release eligibility
remain unchanged. A9, A11 and A19 remain OPEN.
