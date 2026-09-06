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

## 2026-09-06 experiment-retention deterministic reseal

Tested code head `06174b610277f26bfccecfda7c7b2c47428de8fa` adds the
dry-run-first experiment archival tool, adversarial retention tests, the narrow
upstream-process documentation attribution check, and detailed cleanup records.
It does not establish new guest behaviour or change any capability threshold.

- [CI 34045844404](https://github.com/Ketchio-dev/bridgevm/actions/runs/34045844404)
  completed at that exact SHA with a **failure** conclusion. All independent
  jobs succeeded: clippy, rustfmt, MSRV 1.85, native Venus tests, macOS app
  suites, Windows NVMe workload compilation, Linux cross-compilation, macOS
  15 and 26 tests, Venus feature checking and structural budgets. The optional
  latest-stable advisory was skipped.
- The sole failed job, `capability and documentation drift` (`101520619448`),
  rejected the stale registry with `code changed since tested_commit
  (scripts/archive-experiment.py); re-prove A11`. This is a real failed
  workflow, not a green result or a waiver.
- [Security 34045844400](https://github.com/Ketchio-dev/bridgevm/actions/runs/34045844400)
  succeeded at the same SHA. All five jobs passed: live-gate policy, graphics
  compatibility claims, fuzz corpus smoke, supply-chain policy and loom
  interleaving models.
- The precommit full local `scripts/check-project.sh` passed with new files
  staged, including 13 retention tests and four attribution-policy tests.
  ShellCheck was unavailable and the existing Swift shim suite reported one
  skip. Crucially, freshness compares committed `tested_commit..HEAD`, not
  staged content: that precommit PASS did not prove postcommit freshness.

The registry/documentation-only follow-up points `tested_commit` to this code
head, updates A11's measured narrative, and regenerates the capability blocks.
The code-path freshness guard is unchanged. The follow-up must pass the full
local project check and its own exact-SHA hosted CI and Security before it is
called a green deterministic seal. Its run IDs and final results can be found
on the hosted commit checks; pending results are recorded in the operator plan
rather than described here as already successful.

The full local check on the registry/documentation-only working tree completed
within 300 seconds with `project check: PASS` and exit 0. Registry freshness
and all three generated blocks passed. ShellCheck remained unavailable; the
shim suites reported 419/419, 245 passed with one skipped, and 62/62. These are
shim results, not Apple XCTest or live Windows guest evidence. Hosted checks
for the forthcoming committed seal remain a separate required result.

Current AGENTS instructions supersede the historical physical-Mac T0 venue
above: deterministic checks run locally within 300 seconds or on GitHub-hosted
Actions. No ordinary deterministic check is submitted to the live queue.
This work is not a release, a packaging receipt or a live guest gate. Failed
T15/T16 campaigns and T17 pilots remain failed; all capability states, open
defects and the Engineering Preview product state remain unchanged.

## 2026-09-06 hosted packaging dependency drift

The registry seal `8d4eaab52d75fad17c9c9f59bdce23baa726de91` passed hosted
CI `34048024431`, Security `34048024411`, and the full local project check.
However, artifact-only [Release 34052275951](https://github.com/Ketchio-dev/bridgevm/actions/runs/34052275951)
failed during packaging. Its source boundary and renderer build passed, but
Homebrew supplied swtpm 0.10.2 while the bundle contract requires 0.10.1. The
bundler correctly rejected that source; this run produced no verified package.

The workflow correction installs the TPM pair from the official upstream Homebrew core
commit [918b0d7fd69f045c93f45ce5caa3940735c52476](https://github.com/Homebrew/homebrew-core/tree/918b0d7fd69f045c93f45ce5caa3940735c52476),
which defines swtpm 0.10.1 and libtpms 0.10.2 with source and bottle hashes.
The hosted runner's existing tap is temporarily moved aside and restored on
exit; the pinned tap is retained in runner temporary storage. Unexpected
symlinks/non-repository paths and non-hosted execution are rejected. Automatic updates and API
formula resolution are disabled for this installation; the resolved commit,
installed executable version and libtpms Cellar path are checked explicitly.
The existing bundle version, dependency-closure, license and signature gates
remain unchanged. No installed dependency on the user's Mac is modified.

This pins the TPM formula snapshot, not every packaging dependency or the
hosted runner image, and is not a bit-reproducible-build claim. The corrected
workflow still needs exact-SHA hosted CI/Security and a successful artifact-only
Release run before its package can enter the T17 preflight. A9 remains open;
no live result, criterion, signing class or product state is promoted here.

The first correction at `17adc661098d10757d1528a642ef42afedbd5d7c` also failed
in [Release 34052972212](https://github.com/Ketchio-dev/bridgevm/actions/runs/34052972212):
the runner already had a core checkout, and `test ... && test ...` did not
abort on its first false command under Bash `errexit`. Git reinitialized the
existing repository and then rejected adding its existing `origin`. No pinned
installation or package was proven. The follow-up uses an explicit rejection
branch and a preserve/restore transaction instead of assuming an absent tap.

## Verified hosted package and blocked T17 pilot

The follow-up `e013d1bc82ffb741c979922d471f8ca3b252d6fe` passed
[CI 34053212350](https://github.com/Ketchio-dev/bridgevm/actions/runs/34053212350),
[Security 34053212343](https://github.com/Ketchio-dev/bridgevm/actions/runs/34053212343)
and [artifact-only Release 34053212006](https://github.com/Ketchio-dev/bridgevm/actions/runs/34053212006).
The full local project check also passed. These results establish the pinned
TPM installation and package pipeline at this exact SHA, not Windows guest
installation or release readiness. No tag or public release was created.

Downloaded artifacts were checked against their retained `SHA256SUMS`:

| Artifact | SHA-256 |
| --- | --- |
| `BridgeVM-release.json` | `f0ed3a1f1ea5f864ce8264a7532b843453a9f07b58e5c13e7d7fea889e12fef9` |
| `BridgeVM-v1.1.0.dmg` | `731425df992e71c3de47bf3ecac0a71ed7f3819f1f700e979d0497aaa32bff7b` |
| `BridgeVM-v1.1.0.tar.gz` | `72389192b7caa0e8abf3ee1458aac287f12ee9ebd037de5575ea566ea1dc72b0` |

The release contract names exact source SHA e013d1bc, General Preview,
Engineering Preview, 3D-off installation, no included Windows kernel driver,
no product injection and no TESTSIGNING requirement. Its signing declaration
is ad-hoc, not Developer ID or notarized. The 117-entry archive passed a
path/type/duplicate/relative-symlink audit before extraction into a new local
directory. Downloaded bytes then passed deep strict app signature validation,
both packaged HVF entitlements, nested E2E helper identity/signature, TPM
runtime dependency/hash/signature validation, firmware and wimlib provenance,
and third-party notices (55 Rust dependencies, 12 frameworks and six LGPL
dynamic consumers). Private inputs and extracted applications remain local.

All ten T17 inputs were rehashed before the one-lane diagnostic
`t17-e013d1bc-hosted-pilot-r1` was submitted. Its sealed manifest SHA-256 is
`6146db63548d6b7b5b9819cecee10eded7728722bc9d678103a984cc5c01b8e4`.
The physical-Mac queue ran the exact e013d1bc job on 2026-09-06 from
19:06:11 to 19:06:53 UTC. **The pilot failed.** The private lane reported
`accessibility-untrusted`, `ui_frontend_automated=false` and
`cleanup_verified=true`. The public receipt reports `outcome=failed`,
`failure_code=integration-failed`, zero passes from one run,
`claim_eligible=false` and `worker_cleanup_verified=true`. Its retained public
receipt SHA-256 is
`2020ad340b9bb4df9cdc5956bb0a5fa531303244d937b6a72778a44fc6cf4246`.
No remaining product/helper/HVF runner process was found after completion.

The exact nested helper has the ad-hoc designated requirement CDHash
`7C76137B5C6058FB9B2A11BBAEB31D877E6E4238` (hexadecimal bytes, not a Git commit).
The next live attempt requires
the user to authorize that downloaded helper in macOS Accessibility settings;
an older same-name entry does not establish trust for these bytes. Do not
edit TCC databases, bypass permission prompts or re-sign this sealed artifact.
Do not repeat the pilot without a changed prerequisite. This is no installed
Windows proof and no substitute for the fixed three-lane campaign. A9 and
all other open criteria remain open; the product state is unchanged.

## F4 presented-frame observation channel

Code head `cf6fb990286a5a220bd843accd6c8fe7ebd3ef5f` corrects the remaining
F4 capture caller: it no longer accepts the 2D virtio-gpu checkpoint that the
retained all-black scene had falsified as presentation evidence. It enables
synchronous IOSurface export and calls the existing generic active-CGL capture
with a five-second limit. Newly presented seed advancement and nonblack full
BGRA content remain required. Raw bytes, PPM and capture metadata are retained;
the legacy PPM/hash path remains available to the observation report. A failed
capture returns failure instead of falling back to the old framebuffer.

The closure policy smoke now requires this channel and rejects the obsolete
SNAPSHOT/checkpoint path, while preserving the guest-proof and OCR checks.
Running the smoke in the actual whitespace-containing checkout also exposed
an unquoted CLI command substitution, which was corrected. Focused policy and
pure BGRA/geometry/stale/all-black tests passed. A synthetic adapter check
verified successful copy/hash and refused-capture propagation. Existing file
budgets were not raised. OCR remains observation, never the B6 pixel-mask gate.

[CI 34054552449](https://github.com/Ketchio-dev/bridgevm/actions/runs/34054552449)
completed with every independent required job successful, the optional advisory
skipped, and only capability/documentation freshness failed against the older
tested commit. This remains a failed workflow. [Security 34054552486](https://github.com/Ketchio-dev/bridgevm/actions/runs/34054552486)
succeeded. The full precommit local project check passed, but its committed-head
freshness comparison did not include the uncommitted script change. The
registry/documentation-only follow-up therefore points to this exact code head
and requires its own full local and hosted checks. No freshness guard changes.

No corrected-channel live Notepad scene or B6 matrix has yet been measured.
The next investigation must capture that exact visible failure before proposing
a draw-path cause. B6 stays open with unchanged 3/3 observations at each of
three resolutions and three scales, verified pixel masks and the 10% frame-time
bound. T17's separate Accessibility prerequisite is unchanged.

## Closure guest-launch protocol and nested policy output

Code head `24a3b5c1617560339c82bd21a73be8a56a55bfda` replaces the inline
Notepad Start-Process call with a share-delivered CRLF PowerShell script,
invoked through `-File`. It launches through `Invoke-CimMethod Win32_Process
Create`, rejects an unsuccessful return or missing PID, and emits the named
completion file after the existing three-second settle. The host requires that
file to return through the agent share; it no longer ignores launch failure.
The new eight-line payload and 36-line static test have actual-size structural
registrations, without raising existing ceilings. Nine rejection mutations
passed, including a corrected positive-newline requirement for CRLF validation.
This static evidence does not prove CIM execution inside Windows.

[Security 34055674449](https://github.com/Ketchio-dev/bridgevm/actions/runs/34055674449)
failed at that code head with a real Broken-pipe error: a nested smoke emitted
PASS, the parent `grep -q` closed the pipe, and later output failed. This is
not a registry-only failure and is not reclassified as success. Code head
`59eb783ea1bd6b95c838351844c8975e6bd3a169` changes the five nested output
consumers to draining grep with output redirected to `/dev/null`, retaining
pipefail. It also quotes two executable-path substitutions exposed by the real
whitespace-containing checkout. A late-output success test and a producer that
prints PASS then exits 7 demonstrate both pipe draining and error preservation.
The actual complete live-policy deterministic suite passed all 99 checks.

[CI 34055944404](https://github.com/Ketchio-dev/bridgevm/actions/runs/34055944404)
completed with every independent required job successful, including macOS app
suites, and the optional latest advisory skipped. The workflow remains FAILED:
capability/documentation drift rejected the stale tested commit at
`scripts/refactor-budgets.tsv`. [Security 34055944527](https://github.com/Ketchio-dev/bridgevm/actions/runs/34055944527)
succeeded at the exact corrected code head. The subsequent full local project
check completed within 300 seconds with only capability registry failing;
every other step passed. That overall result is FAIL, not PASS.

This registry/documentation-only seal points to the corrected code head and
requires its own full local project check and exact-SHA hosted CI/Security.
It does not weaken freshness or retrospectively change either failed workflow.
The existing ten-input T7 verifier accepted the reconstructed private inputs,
but no new live closure run has been submitted. This is preparation for a new
B4-derived diagnostic, not a replay of the absent August T7 experiment. The
attested probe source remains `69dea55a1902e3c11834f87bc19b9995e79a504e`,
separate from the corrected harness source. Input validity and deterministic
tests do not close B6's live glyph matrix or T17's Accessibility blocker.
All open criteria and the Engineering Preview product state are unchanged.

## T7 input access and immutable-source clone permissions

The preceding registry/docs seal `be239e3f245c37cd906c8741c16e64e662a35e3a`
passed the full committed-head local project check,
[CI 34056586757](https://github.com/Ketchio-dev/bridgevm/actions/runs/34056586757)
and [Security 34056586708](https://github.com/Ketchio-dev/bridgevm/actions/runs/34056586708).
The optional latest advisory was skipped. Two subsequent private T7 diagnostics
used that exact harness and the separately attested probe source
`69dea55a1902e3c11834f87bc19b9995e79a504e`. Neither produced a guest sample.

Job `t7-be239e3f-b6-observation-r1` ran from 20:04:02 to 20:04:44 UTC on
2026-09-06. Its outcome was `refused-input`, with zero samples and no injector
boot or presented capture. The worker's log reports `Operation not permitted`
when shasum reads the agent source in the Desktop checkout. Successful input
verification in an interactive terminal did not establish worker access.
The failed public receipt SHA-256 is
`f199ef223af0d080363a518887bfadb8c44448ed2eab0270d5a6eef831e3d19e`.
The agent source was then copied to the private campaign-input directory with
mode 400 and identical SHA-256
`b7820834cee3f34ab895c88b31f9c6c0e96d8052c8b34ce5db691e2e968e844c`.
Only that path changed in the new manifest. No TCC settings, signed artifacts,
canonical media or expected content hashes were changed.

Job `t7-be239e3f-b6-observation-r2` ran from 20:06:45 to 20:08:43 UTC.
Actual worker verification accepted all ten inputs and both embedded identity
records. The module identity check also succeeded. The manifest SHA-256 was
`cf1e062838fa386e1812be4b6fc5ec75c0cf1be75e0a3e1e42de2ab0f7e3ebc6`.
This attempt then exited 101 before injection, with `failed-before-receipt`,
zero samples and no F1-F4 evidence. Its public receipt SHA-256 is
`657108f9cb6e4f30080cf38301f668d5b95ca3ef07d8d1a6b883083c536ff094`.

The failing state is explicit: injection/run.log reports a panic at
`boot_media_setup.rs:34` attaching the cloned NSID2 target, with `Permission
denied (os error 13)`. Both before/after target stat show mode 400 on the cloned
disk; cloned vars also have mode 400. Copying immutable sources preserved their
read-only mode. HVF/GIC creation and renderer initialization occurred, but this
does not demonstrate that a guest instruction ran or a Windows frame existed.
Harvested historical guest files are not new-run injection evidence.

Correction `40afd096db5376f4c5324bd273e5a3e0438855f6` extracts the injection
stage copy operations to a small helper and changes only the three per-job
copies to mode 600. Disks still use APFS `cp -c`; vars remain a separate copy.
Copy or chmod errors fail closed, and the existing clone/source hash checks
remain. Canonical input modes and contents are not modified. The fixture test
executes the actual helper with mode-400 sources, checks different inodes and
destination modes, writes to the clones and checks unchanged source bytes.
Five mutations reject omitted chmod, source chmod and each missing destination.
The new eight-line helper and 51-line test have actual-size budget entries;
existing structural ceilings were not raised. Focused smoke and full precommit
local project check passed within 300 seconds. Committing correctly makes A11
freshness stale until independent hosted checks and a separate evidence seal.

No corrected live attempt has yet run. These records establish two distinct
pre-boot failures and a deterministic staging correction, not Windows glyph
correctness, F1-F4 success, or any promotion of the product state.

[CI 34057283267](https://github.com/Ketchio-dev/bridgevm/actions/runs/34057283267)
completed with all independent required jobs successful, including every macOS
app suite step. The optional latest advisory was skipped. The overall workflow
remains FAILED because capability/documentation drift rejected the stale tested
commit at `scripts/live-gates/run-windows-closure-tier.sh`.
[Security 34057283256](https://github.com/Ketchio-dev/bridgevm/actions/runs/34057283256)
succeeded at the exact corrected code head. The subsequent committed-head full
local project check also failed only capability registry, with every other
section passing within 300 seconds. This was not an overall PASS.

The registry/docs-only follow-up updates the tested code pointer and retains
both failed live diagnostics and the failed freshness workflow. Its own full
local project check and exact-SHA hosted CI/Security must pass before the next
live submission. Deterministic staging success is not live Windows proof.

## Third T7 diagnostic: injection succeeded, proof boot did not

Registry seal `bdad4cbc9426066bd4aed2045f08ad9e7dde74ba` passed the full
committed-head local check, [CI 34057653381](https://github.com/Ketchio-dev/bridgevm/actions/runs/34057653381)
and [Security 34057653391](https://github.com/Ketchio-dev/bridgevm/actions/runs/34057653391).
Job `t7-bdad4cbc-b6-observation-r3` ran 20:26:47–20:33:22 UTC on 2026-09-06.
Its terminal receipt is failed, one sample, zero passes; F1/F2/F3 are false
and F4 blocked. Public receipt SHA-256:
`1417fb1f7e26d6e5d46f1dfccf0d84d646188ae032df85f573212a4bfaabc1e5`.

The actual staged disk, vars and injector had mode 600. Injection returned 0,
injector boot and module identity were verified, and source hash rechecks
passed. The immutable retained prepared disk has SHA-256
`a015d92bb5be37e9c6cb0f14d54d53293753f5a5461d1c40dc39c9569d278d2e`;
its paired target vars remain
`bec224d27c8681d2db69583e933e2d99b6fa5265d91d37373cb7a2c8b71853cd`.
Proof used another disk clone and distinct vars copy. This establishes that
the earlier clone-permission failure was cleared, not that Windows proof passed.

Proof RAMFB checkpoints at 1, 5, 15, 30 and 60 seconds had checksum
`af552b4d7621db7e`. Visual inspection of the 30-second checkpoint showed
TianoCore and Start boot option. It was not an active CGL Notepad capture.
The existing boot-progress watchdog stopped the run after 120000 ms of low
progress, four exits in the window and 72232 total exits before termination.
The agent-service wait failed; no glyph scene was reached.

Final owning-thread evidence identifies PC `0x1bf33ba04` in ArmCpuDxe with
preceding WFI `0xd503207f` and current RET `0xd65f03c0`. CNTV_CTL was 1,
virtual timer mask false, CVAL `0x74da4e607e3`, guest count `0x74da5107feb`.
GICR enable was `0x6c000000`, pending and active zero, PMR `0xf8`, IGRPEN1 1.
The owning-thread classifier reported parked, stall=false, timer PPI enabled
but not pending. The generic watchdog stall label does not override this.
There were zero virtual-timer exits and no drained MSI-X/SPI events. A bounded
three-second host stack sample also showed the primary vCPU waiting inside
Hypervisor, but neither observation proves a lost timer or its cause. The
supplemental private sample SHA-256 is
`15b1dd5e9ba4168f4f50331a79f74cc26b29616a449df7e366979d25a7ff9975`.

Comparison with retained B4 job `20260829-215356-7576-17586`, lane run1,
generation 0, narrows but does not isolate the cause. That lane recorded agent
service start at t=18836 and guest shutdown, also with zero virtual-timer exits
(14894 MSI-X drains). Both use a single NVMe target, no NSID1 placeholder,
6144 MiB RAM, four CPUs, xHCI, synchronous IOSurface scanout and the same source
vars identity. Firmware blobs in B4 harness
`080462846acbe4cb784bd9b532d7cd39921aa549` and the r3 harness are identical
by Git object comparison. Thus neither a different recorded NVMe topology nor
a different committed firmware blob explains this pair of observations.

Important confounders remain: B4 boots the original prepared disk beginning
`7385d200`, while r3 boots its reinjected derivative beginning `a015d92b`;
the harness/code generations differ substantially. B4 uses exit-on-reset,
200 ms HID pacing and DCI5 tracing, whereas r3 uses in-process reboot handling,
default 30 ms pacing, boot-progress kill, Venus-start tracing and PPM export.
Share destinations and watchdog limits also differ. Do not label this a
controlled A/B test or infer a disk, timer, renderer or firmware fix from it.
The next diagnostic should isolate prepared media from execution configuration
with sealed inputs and independent clones; it must not reduce the B6 matrix
or substitute a pointer smoke for glyph correctness. Historical failures and
current OPEN capability wording remain unchanged.

## Sealed two-media diagnostic: implementation and operation

The `d1-windows-media-comparison` queue route runs the same existing closure
proof twice, first on original media and then on reinjected media. It does not
inject either source again. Both observations use one sealed binary, identical
firmware, vars contents, renderer, driver tree and execution settings. Each
gets a separate writable same-volume disk clone and vars copy. A lane's ordinary
proof failure does not suppress the second observation, but changed inputs or
media still in use stop the diagnostic. This fixed-order pair cannot establish
repeatability or distinguish order effects; it is not the B6 acceptance matrix.

The private UTF-8 TSV manifest requires these six exact two-column rows:

```text
schema	bridgevm.windows-media-comparison.v1
purpose	diagnostic-only
claim_eligible	false
order	original,reinjected
sample_count	2
profile	windows-closure-proof-v1
```

It also requires exactly eight three-column asset rows, each containing its
key, normalized absolute path and lowercase SHA-256: `original`, `reinjected`,
`vars`, `binary`, `firmware`, `viogpu_dir`, `virglrenderer`, `moltenvk`.
Do not include private paths or media in git. The two image hashes must differ;
aliasing assets, duplicates, missing rows and changed contents are refused.
Driver identity uses the filename-sorted SHA-256 listing convention, rejecting
symlinks and ambiguous filenames. The worker's copied sealed binary is used
instead of reopening the caller's binary path. Firmware is authenticated from
the exact worker checkout's fixed `edk2-aarch64-secure-code.fd` path, not from
an arbitrary replacement provided by the caller.

After final exact-SHA CI/Security and worker compatibility checks, submit with:

```sh
scripts/live-gates/bridgevm-live submit d1-windows-media-comparison \
  --sha <full-verified-harness-commit> \
  --input-manifest <absolute-private-manifest.tsv> \
  --job-id <unique-diagnostic-id>
```

The runner is asynchronous through the physical-Mac queue, not a foreground
long test. The binary's attested source commit must be recorded separately
when it differs from the harness commit. The output `diagnostic` directory
must be new; the queue adapter creates it inside the existing job directory.
Per-lane proof logs, original framebuffer data and any actual CGL captures stay
private. A boot framebuffer is not substituted for the glyph scene. Sources
and copied bytes are authenticated before each launch and inputs are checked
again after each lane. Successful idle lane scratch is removed; unsafe or
canceled scratch is retained for investigation, never treated as disposable
while a guest may still own it.

All result flags `pass`, `claim_eligible`, `criterion_pass` and
`capability_promotion` remain false, including when both proof processes return
zero. `diagnostic-complete` means both observations finished, not that any
criterion passed. The adapter deliberately returns nonzero to the generic
worker so its coarse queue status cannot advertise a criterion pass; inspect
the diagnostic outcome and individual proof exit codes. Cancellation produces
`canceled`, even when a partial private receipt already exists. Publication
checks identity, fixed counts/order, safe field types and non-promotion flags;
private paths and arbitrary lane data are not included in the public summary.

The deterministic implementation checks cover 19 input rejection cases,
actual APFS fixture copying with fake proof success/failure, source mutation,
busy media, sealed CLI submission, dispatch refusal, cancellation finalization
and publication. A separate dummy guest holds a real cloned file open while
the actual worker process-group helper cancels its parent and descendants.
That test checks terminated processes, retained scratch, unchanged original
bytes/modes, no second lane after cancellation, and a claim-ineligible canceled
public receipt. It also rejects source changes between sealing and copying.
These are deterministic tests, not Windows execution evidence.

The first actual cancellation fixture failed: `kill -0` succeeded just before
process exit, the subsequent `ps` yielded empty state, and the common helper
classified empty state as alive, returning cleanup status 126. Code head
`e671ff0f31a0a330d2da0b63b8f8d07fb3c97eeb` requires a nonempty, non-zombie
state. Empty/zombie/live state fixtures and actual group cancellation passed
after correction. Temporary shell tracing was removed. The installed worker
checkout must also receive this common helper correction while idle; merely
fetching a new per-job worktree does not update the long-lived worker's helper.
No diagnostic live pair has yet been submitted or measured.

At that exact final code head,
[CI 34059998099](https://github.com/Ketchio-dev/bridgevm/actions/runs/34059998099)
completed with all independent required jobs successful and the optional latest
advisory skipped. Its overall outcome remains FAILED: capability/documentation
drift rejected the stale tested commit. [Security 34059998109](https://github.com/Ketchio-dev/bridgevm/actions/runs/34059998109)
succeeded. Full pre-seal local project checks completed within 300 seconds with
only capability registry failing. Those are not overall passes. This final
registry/docs-only seal updates the code evidence pointer without altering the
freshness guard and requires its own full local and exact-SHA hosted checks.
Both complete private image hashes and all six common assets were authenticated
by the actual diagnostic verifier before submission; input validity is not live
proof. The installed worker update and actual media comparison remain pending.
