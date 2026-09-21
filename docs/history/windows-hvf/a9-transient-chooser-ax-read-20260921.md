# A9 transient chooser AX read — 2026-09-21

Historical evidence record. The capability registry owns current wording and
status. This checkpoint does not close A9 or A11, product state remains
Engineering Preview, and 3D remains outside General Preview and v1.

## Physical r23 result

Exact-main commit `23c11d5c3ec38541cfefa7425afcd505429f7a4c` completed
all 45 hosted push workflows. Its exact-source Apple Development-signed app
passed deep strict signing, entitlement, nested-helper, dependency, notice,
immutable-input and LaunchServices Accessibility preflight checks. The sealed
input manifest SHA-256 was
`b9a9cbbe85627a052bee2cfe201e056f472687c9fa6be7dd6f71404e2ca15aac`.

Physical pilot `t17-23c11d5c-share-chooser-r23` passed artifact preflight,
exact VM creation, source preparation, Windows installation and Microsoft-only
Secure Boot provisioning with 3D injection disabled. The authenticated lane
retained the finalized 64 GiB disk, 64 MiB variables and install receipt hashes.
It then failed before first READY/PONG while opening the runtime host-share
chooser. The exact private failure was `input-selection-failed` at
`stage=initial-panel-check`: the chooser AXIdentifier read returned generic
AX failure `-25200`.

The authenticated lane verified cleanup. The outer public receipt retained
`product-model-failed`, `criterion_pass=false`, `claim_eligible=false`, and
`capability_promotion=false`. The strict public receipt passed repository
verification and has SHA-256
`f0a7f69213aa58de5568fb1c53205e9a9674a9d52e8002455d95bcbc6faf635a`.
The authenticated private lane result has SHA-256
`90299554dfa9e92dbc6de8aaadbbb35fcc4cb893c1bc44434215c390cbebe424`.
Private paths, media and guest state remain outside git.

## Bounded correction

Source `7774590c23b0356c12e68d7061c283c9f6bb91d1` classifies only generic
failure, invalid-element and cannot-complete AX read results as transient. It
reacquires the application root and complete bounded graph for at most ten
attempts paced by 200 ms. Unsupported AX errors still fail immediately, and
exhaustion retains the exact final blocker; absence, role mismatch and
ambiguity remain failures.

Four focused contracts cover graph reacquisition, the observed generic failure,
cannot-complete, unsupported errors and bounded exhaustion. The complete
ProductE2E suite and structural budgets passed. The complete local project
check passed every executable, app, security, documentation and structural
step; its retained log SHA-256 is
`d9c3df03e65686b19bc166eb16b2a72096c7ce6b55382ecb5f9f86417ae5a05a`.

## Limits

The r23 pilot is a failed live single run. It proves installation and Secure
Boot stages for that lane, but it does not prove first READY/PONG, a usable
Windows desktop, integration behavior, shutdown, clean-machine completion,
installed-disk import or A9. The correction is deterministic host evidence
until a newly signed exact-source artifact passes another physical pilot. No
threshold or pass condition changed, and T19 remains required.
