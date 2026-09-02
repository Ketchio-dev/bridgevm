# A11 exact-head regression seal (2026-09-01)

This record separates the tested code head from the registry-only seal. It does
not turn an expected freshness failure into a passing run.

## Tested code head

- Commit: `8171fdb5e4c64b7092f68818f3702f824eb0b435`
- Studio T0 job: `20260902-021748-t0-8171fdb5-r2`
- Host: `Mac17,9`, macOS `26.5`
- Public receipt outcome: `failed`, `pass=false`
- Retained `check.log` SHA-256:
  `f74cbafad43de1bd64fc0b2f745016e76c8b76926d5a643098ebe9c913b1f110`

The deterministic project check ran to completion. Its only failed step was
`capability registry`, which rejected the deliberately stale `tested_commit`.
Every other project-check section passed. The registry-only seal updates that
pointer and regenerates the derived status blocks; its own hosted checks must
still be green before the seal is treated as complete.

An earlier T0 job, `20260902-021510-t0-8171fdb5`, was interrupted when the
queue worker was forcibly restarted after claiming it. Its worktree and output
directory moved while the child check was still running, so it produced no
published receipt. That failed experiment remains in the queue record and is
not used as evidence.

## Hosted checks at the tested code head

- Security and quality run `33582509854`: `success`. All five jobs passed:
  loom, graphics compatibility claims, fuzz corpus smoke, supply-chain policy
  and live-gate policy.
- CI run `33582509937`: `failure`. All independent code, build and test jobs
  passed; only `capability and documentation drift` failed because the registry
  and its generated blocks still named the earlier tested commit.

The code-head CI failure is retained. It is not a green seal and cannot promote
live evidence. The registry-only commit must be pushed and its exact SHA must
receive successful hosted CI and Security results.

## Explicitly open at that seal

The hosted QMP campaign at code head run `33582528722` was exploratory until the
same exact green seal passes the full 20/20 negative control and 60/60 workspace
rounds and its artifact is independently verified. At that seal, the audio
teardown 10-run campaign, M1/M2/M3 clean-machine matrix, 20-workload
compatibility matrix, A9 Windows kernel acceptance and complete app-driven
install/recovery E2E were also open. Product state remained Engineering Preview.

## Release-preparation reseal

The later release-preparation code head is
`99a3dfffa58485e402c685875d8d2c1b3752dbe1`.

- Studio T0 job `20260901-231136-2069-30402` ran the complete deterministic
  project check on `Mac17,9`, macOS `26.5`. Its public outcome is `failed` and
  `pass=false` because the intentionally stale capability registry was the
  sole failed step. Every other section passed. The retained `check.log`
  SHA-256 is
  `bebacbe5cc86a2cd60af6b7f1c6d625427028b7ceafb9cadb9a844abc9da4fc2`.
- Security and quality run `33586100100` passed all five jobs at that exact
  head.
- CI run `33586100088` passed every independent code, build and test job. Only
  `capability and documentation drift` failed against the stale registry.

Release dry-run `33585644986` at earlier head
`12831bf27446b979a41db2419686e090512d3c53` was canceled and is not promoted.
It exposed that the release workflow's shallow checkout let the source-boundary
step miss stale capability evidence. The final code head requires complete Git
history and tests that invariant. A separate invalid manual T0 submission used
a guessed unknown commit, produced no receipt and remains in the queue record;
the final head rejects malformed or unknown commits before burning a job id.

The registry-only commit that records this section must itself receive green
hosted CI and Security results before it becomes the release seal. B7 was
separately proven by its fixed ten-run receipt. A9, B6, B8, B9 and B10 remain
open, and the product remains Engineering Preview.

## macOS 26 reconciliation correction and final reseal

The final tested code head is
`4fa4214931aac0cdf090a7b24943df3f4f349160`. Exact-head CI run
`33587382844` passed every independent code, build and test job on macOS 15
and 26. The corrected
`reconcile_children_records_agent_update_notice_as_runtime_metadata` test
passed in the macOS 26 workspace run, and the daemon suite passed 73/73 there.
Only `capability and documentation drift` failed against the intentionally
stale registry. Security and quality run `33587382867` passed all five jobs.

Studio T0 job `20260901-233149-35337-15851` ran the complete deterministic
project check on `Mac17,9`, macOS `26.5`. Its public receipt remains
`outcome=failed` and `pass=false`; the sole failed section was the deliberately
stale capability registry. Every other section passed, including the complete
Rust workspace, Venus, probe, Swift, shim and 21-case installer checks. The
retained `check.log` SHA-256 is
`c1319cd75b712adb3d854924e8cab00ff82669c1b55db478c254af6b3315a40a`.

Before submission, the exact reconciliation test also passed 20/20 sequential
local runs and the full daemon suite passed 73/73. These are focused automated
signals, not live guest evidence. The registry-only commit that records this
section must itself pass hosted CI, Security and a complete exact-SHA T0 before
it is a green release seal. B7 remains separately proven. A9, B6, B8, B9 and
B10 remain open, and the product remains Engineering Preview.
