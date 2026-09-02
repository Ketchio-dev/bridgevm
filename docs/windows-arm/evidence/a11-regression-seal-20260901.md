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

## Packaged HVF entitlement correction and 2026-09-02 reseal

Independent inspection rejected release dry-run `33588078623` even though its
workflow had succeeded. The packaged `hvf_gic_boot_probe` carried
`com.apple.security.hypervisor`, but the product's preceding `hvf-runner` did
not. That artifact remains a failed experiment and is not a release candidate.

Tested code head `7d92a62517c35d746cc84a71452c7602ec2064ad` routes release
packaging through the locked runner signer, selects
`HvfRunner.release.entitlements` for `--release`, excludes debug
`get-task-allow`, and verifies both packaged HVF executables after the complete
app is signed. The deterministic self-test accepts a release-entitlement pair
and rejects both a missing entitlement and the debug entitlement. It also
rejects the retained earlier dry-run app.

- CI run `33589168317` passed every independent code, build and test job on
  macOS 15 and 26, including the new packaged-entitlement self-test. Only
  capability/documentation drift failed against the deliberately stale
  registry.
- Security and quality run `33589168261` passed all five jobs.
- Studio T0 job `20260902-000111-70656-2849` completed on `Mac17,9`, macOS
  `26.5`. Its public receipt is still `outcome=failed`, `pass=false` because
  capability registry freshness was the sole failed step. Every other project
  check passed, including the complete Rust workspace, Venus, probe, Swift,
  shim and 21-case installer checks. Retained `check.log` SHA-256:
  `4ea792a3f862ebb6f77d220a003af64725409243762f2050c3508fef4f9c887f`.
- Release dry-run `33589193072` stopped at the full-history source boundary
  before producing an artifact because the registry was stale. It is not
  packaging evidence.

The registry-only commit recording this correction must itself pass hosted CI,
Security and a complete exact-SHA T0 before it is a green release seal. Product
state remains Engineering Preview; A9, B6, B8, B9 and B10 remain open.

## T17 Accessibility helper identity reseal

Tested code head `47fefe5fefe5f1a2946f9685af35ed1a7ec9bb40` replaces the
bare `BridgeVMProductE2E` executable with a signed nested APPL bundle whose
fixed bundle identifier is `dev.bridgevm.product-e2e`. The package, release and
live-tier boundaries reject the former bare path, a changed identifier, a
missing signature or a malformed nested app. This is a packaging correction;
it is not live proof that macOS Accessibility accepts the new identity.

- The focused Swift product-E2E suite passed 9/9, and the deterministic T17
  contract passed all 41 checks at the exact committed head.
- CI run `33600999709` completed with every independent code, build and test
  job successful on macOS 15 and 26, including the T17 contract in the macOS
  app suite. Only `capability and documentation drift` failed because the
  registry deliberately still named the preceding tested code head.
- Security and quality run `33600999725` passed all five jobs.
- Studio T0 job `t0-47fefe5f-a11-preseal` ran the complete deterministic check
  on `Mac17,9`, macOS `26.5`. Its public outcome is `failed`, `pass=false`
  because registry freshness was the sole failed section; all remaining
  sections passed. The retained `check.log` SHA-256 is
  `c8a39cc4245da4ba42610a4cd3ed8edce1e1bbf094335381cf6c554c5f53bda6`.

The registry-only commit recording this evidence must itself pass hosted CI,
Security and an exact-SHA T0 before it becomes a green release seal. The three
earlier `accessibility-untrusted` pilots remain failed evidence. A new packaged
artifact and live pilot are still required; A9, B6, B8 and B9 remain open and
the product remains Engineering Preview. B7 and B10 remain separately proven.

## T17 fresh-library product entry correction

Exact package pilot `t17-86c5fd68-v1.1.0-pilot-r1` did not pass. Its ten
sealed assets and package preflight succeeded, the Accessibility frontend
started, and worker cleanup was verified, but no product stage completed. The
lane reported `ui-element-missing`; public receipt SHA-256 is
`086f88b7c6221977025c724ee54a4083f44b976fc6d847f14cecac0f420b74e4`.
The failed result remains failed evidence.

The failing state exposed a product-automation mismatch. An empty isolated
library renders `FirstRunView`, while the helper waited for
`bridgevm.library.empty.create`, an identifier present only in the different
`emptyState` branch. Code head
`5280ef56cc9f3f017aa36fe1196d9104b8437174` instead enters the existing new-VM
flow through the always-visible `bridgevm.library.toolbar.create` control and
adds a fixed identifier regression test. The focused product-E2E Swift suite
passed 10/10 and the deterministic live-tier contract passed 41 checks.

- CI run `33605274851` passed every independent code, build and test job on
  macOS 15 and 26. Only capability/documentation drift failed against the
  deliberately stale registry.
- Security and quality run `33605274784` passed all five jobs.
- Studio T0 job `t0-5280ef56-a11-preseal` failed only registry freshness; every
  other project-check section passed. Retained `check.log` SHA-256 is
  `95459b2c9b78189a7d499d6abfb3adf4ded3fd496bad1d329087d8ca218ae0fc`.
- Release dry-run `33605319164` stopped at that same fail-closed source
  boundary before building an artifact and is not release evidence.

A local diagnostic package carrying the corrected helper was also not
promoted. Pilot `t17-5280ef56-local-pilot-r1` failed closed as
`accessibility-untrusted`; public receipt SHA-256 is
`f9eb090c5a6bfbcfc852ac98a41553f488dd7fe2be9138f057f92db6e2359bfa`.
Both helper bundles are ad-hoc signed and have different CDHash-bound
designated requirements, so the previous macOS TCC row cannot authorize the
new binary. The old row must be removed and the exact corrected helper added
after user authentication before another pilot is submitted. A9, B6, B8 and
B9 remain open and the product remains Engineering Preview.
