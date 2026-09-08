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
At that implementation checkpoint, no diagnostic live pair had been submitted
or measured. The subsequent completed observation is recorded below.

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
proof. The installed worker update and actual media comparison were still
pending at that checkpoint.

## Two-media diagnostic result, 2026-09-06

Job `d1-e8ceebe1-original-reinjected-r1` completed both observations at harness
commit `e8ceebe12325a0c75f154d384ef315067cd5d20a`. Its public receipt SHA-256 is
`6d0cccccbc1901275136f1a3315d89cd47cde6e2d44d274f65acdc70dbda9d92` and its
sealed manifest SHA-256 is
`e01bfd41467723645db463edd76a7dd83c27c43dd25c5b24658901e5c3d433c2`.
The result is `diagnostic-complete`, with two samples, zero passes, and all four
pass/promotion flags false. Each proof returned exit code 1; each lane's result
records `source_integrity_verified=true`. Completion is not a gate pass.

The original image SHA-256 is
`7385d2005f3c48c40519a2d9cf2b9f975a3763ad8f37e84937d5ddf8429b0f8b`;
the reinjected image SHA-256 is
`a015d92bb5be37e9c6cb0f14d54d53293753f5a5461d1c40dc39c9569d278d2e`.
Both use source vars SHA-256
`bec224d27c8681d2db69583e933e2d99b6fa5265d91d37373cb7a2c8b71853cd`,
with independent writable working copies. Private source images, vars,
per-lane logs and framebuffer artifacts remain outside this repository.

Both observations stopped before the Windows workload:

| Observation | Original | Reinjected |
| --- | --- | --- |
| Proof exit code | 1 | 1 |
| Boot-progress watchdog | 120000 ms, 4 exits in window | Same |
| Final exit count / reboots | 72235 / 0 | Same |
| Final CPU0 PC / LR | `0x1bf33ba04` / `0x478e2a14` | Same |
| RAMFB checksum at 1, 5, 15, 30, 60, 90 seconds | `af552b4d7621db7e` | Same |
| CNTV_CTL / host vtimer mask | `0x1` / false | Same |
| CNTV_CVAL | `0x75f3866db40` | `0x760cc6e12dd` |

The translated PC belongs to ArmCpuDxe, with a preceding WFI instruction
`0xd503207f` and current RET instruction `0xd65f03c0`. The owning-thread GIC
snapshot reports `parked (deadline passed, wake still deliverable)`,
`stall=false`. Both have GICR ISENABLER0 `0x6c000000`, ISPENDR0 zero and
ISACTIVER0 zero. That classifier does not establish why boot made no progress;
it must not be confused with the separate boot-progress watchdog verdict.
Zero virtual-timer exits also occurred in the historical successful B4 lane,
so that count alone is not a distinguishing cause.

The supported conclusion is narrow: reinjection is not necessary to reproduce
this failure shape under the current common execution conditions. This pair
does not identify a timer, interrupt, renderer, firmware or disk defect. Its
fixed original-then-reinjected order and one observation per image do not
establish repeatability. B6 remains OPEN; neither a Windows desktop nor a glyph
correctness scene was demonstrated by these observations.

### Next investigation, not an implemented fix

The historical B4 lane's retained preflight records a release-build completion,
replacement of an existing code signature, and an executable under that job's
private live-work tree. On 2026-09-06 that exact executable path no longer
exists. The inspected receipt, input manifests and preflight do not supply an
executable SHA-256. This is an evidence gap, not proof that every archived copy
is missing, and rebuilding cannot recover the identity of those old bytes.

When using that historical lane, establish its actual executable identity;
its harness commit alone is not proof of which binary ran. Compare one
execution variable at a time on immutable-source clones, retaining the binary,
firmware, vars and configuration identities alongside each result. If the
historical executable cannot be authenticated, a rebuilt historical source is
a new diagnostic artifact, not the old successful artifact. Configuration
differences listed above remain confounders until isolated. Do not change timer
delivery or relax watchdog/acceptance thresholds merely to obtain a passing
run. Preserve this failed pair when recording any later correction.

### Stronger retained control: the identical binary booted Windows

Further inspection found B7 job `20260902-023340-t18-350a7e55`, whose retained
executable was rehashed on 2026-09-06. It is byte-identical to the d1 binary:
SHA-256 `5912a1f291d9935d3d4d2270b37829de543645db2583c3e3f257b9296f1493e8`.
The B7 public receipt SHA-256 is
`5844b7a086e7a3877ec34c0365331f23ea0f2baa2f62b0eab79d1ea0de491081`.
It records ten completed audio-playback/shutdown lanes, not a graphics gate.
All ten retained run-log hashes were recomputed and match their lane results;
each log contains agent service start and PSCI system off. Service-start times
in ordinal order are 22331, 23844, 22328, 24381, 23362, 23577, 21808, 22832,
21584 and 22323 ms. Each lane result reports playback and shutdown passing.

This is a stronger executable-identity control than the older B4 comparison.
It contradicts a claim that these binary bytes cannot boot Windows at all.
It does not prove that they boot the d1 media/configuration, nor does it
establish that the host environment remained unchanged between dates.

| Input or setting | Retained B7 control | Failed d1 pair |
| --- | --- | --- |
| Binary SHA-256 | `5912a1f2…` | Identical |
| Firmware SHA-256 | `b1dc201b…` | Identical |
| RAM / CPUs | 6144 MiB / 4 | Same |
| Disk SHA-256 | `5ad7a304…` | `7385d200…` / `a015d92b…` |
| Vars SHA-256 | `2f0e6892…` | `bec224d2…` |
| xHCI | Disabled | Enabled |
| Virtio GPU 3D | Disabled | Enabled, Venus |
| HDA CoreAudio | Enabled | HDA disabled |
| Watchdog setting | 1500000 ms | 3000000 ms, plus boot-progress kill |
| Agent share interval / maximum file size | 1000 ms / 1024 KiB | 500 ms / 8192 KiB |

The B7 source disk and vars identities are respectively
`5ad7a304cfec4fe9320784b26b4d6895885361ddef2675d2411a759cb54165f8` and
`2f0e68923bf0e4cc1bcfd51a6bb67eb661d48b90aef97ea3b03d4b2805b33ca7`.
The differing disk, vars, devices and observation settings are confounders,
not individually established causes. In particular, the unchanged timer
recovery source and the existence of this identical-binary successful control
do not justify a speculative timer or RAM-allocation rollback.

The next diagnostic should anchor to these authenticated binary bytes and a
currently reauthenticated disk/vars pair, first checking whether the retained
successful configuration still boots, then changing one input at a time.
Each configuration must be sealed and labeled diagnostic-only; the B7 audio
gate must not be relabeled as B6, and the d1 original/reinjected labels must
not be repurposed for unrelated media or configuration comparisons. A new
control's failure is retained and investigated before attributing a changed
lane to its intended variable. No timer fix or graphics promotion is recorded.

## Retained B7 configuration no longer boots, 2026-09-06

The prescribed control was executed. Job `t18-e8ceebe1-boot-control-r1` resubmitted
the unmodified retained B7 input manifest
`3a8cf8ed9fa5d134fac6ef6b345816df01e70a24276fdbe91e826dd79244091a`
at harness commit `e8ceebe12325a0c75f154d384ef315067cd5d20a`. It started
2026-09-06T21:45:25Z and finished 2026-09-06T22:11:52Z. Its public receipt
SHA-256 is `433084bedddb3d65b9d856cc85c9cbfe850edfc5fa33aba5c24a0e1ad6fcb0bc`
and the retained lane log SHA-256 is
`3a5c0e397a1ebe7723bdb717e1645d5ceaee097b87befe86275ae003293e5cc0`.

The control **failed**. The receipt records outcome `failed`, failure code
`lane-failed`, `run_count` 1 against `required_run_count` 10, zero passes, and
`criterion_pass`, `pass`, `claim_eligible` and `capability_promotion` all false.
Elapsed time was 1555000 ms against a 568000 ms ten-lane successful reference.
No criterion is promoted, demoted or rescored by this observation.

Every sealed input matches the successful B7 job `20260902-023340-t18-350a7e55`:
binary `5912a1f291d9935d3d4d2270b37829de543645db2583c3e3f257b9296f1493e8`,
firmware `b1dc201b1382476ca8c8dcbf8c09abc7ae7429c8437e35bffd54bb9b228b750b`,
source disk `5ad7a304cfec4fe9320784b26b4d6895885361ddef2675d2411a759cb54165f8`,
source vars `2f0e68923bf0e4cc1bcfd51a6bb67eb661d48b90aef97ea3b03d4b2805b33ca7`,
host model `Mac17,9`, macOS `26.5`. The recorded device shape lines are
byte-identical between the two runs: 6144 MiB, 4 CPUs, 1500000 ms watchdog,
xHCI disabled by `BRIDGEVM_DISABLE_XHCI`, HDA CoreAudio enabled, ramfb enabled,
virtio-net, virtio-gpu and TPM disabled. The two dates fall in different host
boot sessions; the host booted 2026-09-02 19:35 local, after the successful job.

Lane 1 never reached `BVAGENT SERVICE start` within its 1500 s wait, so the
launcher was terminated and `launcher.exit` records 124; `launcher.out` is empty
and the tier stopped after the first lane. The guest did not reach Windows. The
terminal state is a firmware-stage stall: `pc=0x1bf33ba04`, which resolves to
`ArmCpuDxe` RVA 0x3a04 with `wfi` at the preceding instruction, `lr=0x478e2a14`,
and a frame chain of `DxeCore` RVA 0xdb44 and 0xdfd0, then `BdsDxe` RVA 0xa3ac,
then `DxeCore` RVA 0xb820 and entry 0x16b14. The captured ramfb checkpoint
`0xaf552b4d7621db7e` shows the TianoCore splash and the text `Start boot option`.
The boot-progress watchdog recorded `stalled_for_ms=120000 exits_in_window=4
total_exits=71588 reboots=0 suspect=stalled-before-first-reboot`, and the final
vCPU state is generation 1, PSCI `On`, 77031 exits. `CPSR=0x60000305` with
`I(irq-masked)=0`, `CNTV_CTL=0x1`, `CNTV_CVAL=0x76ee675fd47`, vtimer
`masked=false`. Wake attribution shows 5922 requests, all claimed, with 5921
claimed by `agent-console` and 1 by `reboot-watchdog`, no surplus and no stale
wakes. This is the same stall address, same frame chain and same boot-frame
checksum already recorded for both d1 lanes.

What this closes: the d1 pair's failure is **not** attributable to the reinjected
image, to injection in general, or to any d1-only media identity, because the
retained media and vars that previously produced ten passing lanes now stall in
exactly the same place with the same executable and firmware bytes. The
media-comparison hypothesis that d1 was built to test is therefore not supported.

What this does not establish: it is one live lane, not a gate result. It does not
distinguish a deterministic from an intermittent stall, does not identify a cause,
and does not retroactively invalidate the 2026-09-02 B7 measurement, which stays
sealed to the job and host session that produced it. It does mean the current head
and host cannot be assumed to reproduce that configuration, so the final
no-regression gate must be re-established rather than inherited. The differing
d1 device settings remain confounders that were never isolated. No timer, RAM,
renderer or NVMe change is justified by this observation, and no threshold,
sample count or criterion wording is altered to accommodate it.

## Sealed boot configuration fails 3/3 on the current host, 2026-09-07

The B7 control above is expensive to observe: its lane waits 1500 s before it
gives up. A cheaper independent control exists. Tier `t15-hvf-boot-performance`
boots the same source disk and vars with a 120000 ms watchdog and reports
time-to-desktop, so a stall costs two minutes instead of twenty-six. Job
`20260901-115952-25838-5935` ran that tier on 2026-09-01 from input manifest
`57058d9180f2d9fc99c79472e9c92722b3a5063667497b6bd5f91161fee18852` and passed,
reaching the desktop in 26353 ms with 229856 exits, 4669 exits per second.

That unmodified manifest was resubmitted three times at commit
`e8ceebe12325a0c75f154d384ef315067cd5d20a` as
`t15-e8ceebe1-host-boot-control-r1`, `-r2` and `-r3`, starting
2026-09-07T15:36:04Z, 15:41:57Z and 15:44:42Z. Their public receipt SHA-256
values are
`2926f9337e204c5b94f3fa3353943bafcf3662b102f9aca82f86abb3fd5b28fd`,
`5c449a14b43e9d2cf7968b609818c1550fdcd186e6193d01236053a2e9d41eef` and
`9467c9a47a7af2e774f7fe80160013ef1e0c6e54df1bbf1d1d80359afc7b1a2e`.

All three failed identically with `invalid_reason`
`desktop_not_reached,run_status_nonzero`, no desktop time, and 0 of 8
milestones. Sealed inputs are the ones the successful reference used: binary
`df08c66fb5cbb31786adc5daa50c50b77980d0c6dda1a4e84d298d1f56fa0fbf` built from
`d5d5ed9ce7e621a0b603b1195e872d16ec1f5659`, renderer
`dc596bf39c7a9be4380d4f7d2ae57b8911c880cc719b3eebdccdb9303d6af168`, firmware
`b1dc201b1382476ca8c8dcbf8c09abc7ae7429c8437e35bffd54bb9b228b750b`, disk
`5ad7a304cfec4fe9320784b26b4d6895885361ddef2675d2411a759cb54165f8`, vars
`2f0e68923bf0e4cc1bcfd51a6bb67eb661d48b90aef97ea3b03d4b2805b33ca7`, host
`Mac17,9`, macOS 26.5 build 25F71, AC power at both ends of each run.

The terminal state is the same one the B7 control and both d1 lanes recorded:
`pc=0x1bf33ba04` in `ArmCpuDxe` immediately after `wfi`, reached through
`DxeCore` RVA 0xdb44 and 0xdfd0, `BdsDxe` RVA 0xa3ac, and `DxeCore` RVA 0xb820.
Each 120 s run contains exactly one distinct framebuffer checksum,
`0xaf552b4d7621db7e`. The successful reference recorded that same checksum at
its 1000 ms sample and a different one by 5000 ms, so the failing runs reach
the normal one-second firmware state and then never leave it. The reference
emits the `windows-boot-manager` and `edk2-bds` milestones between its 5000 ms
and 15000 ms samples; the failing runs emit neither. Exit accounting shows the
same shape: about 72700 exits during the first seconds of firmware execution,
then roughly four exits per second for the remaining two minutes. The vtimer is
`CNTV_CTL=0x1` with `masked=false`, and wake attribution reports 474 requests,
474 claimed, zero surplus and zero stale, so the swallowed-fire recovery in
`vtimer_recovery.rs` never triggered and has no bearing on this stall.

This establishes three things and no more.

First, the stall is deterministic on the current host for this sealed
configuration, three runs of three, rather than an intermittent event.

Second, it is not media-specific and not injection-specific. Three different
disk images, two different probe binaries, and three different device
configurations, including one with xHCI, virtio-net and virtio-gpu 3D enabled
and one with all three disabled, stall at the same instruction with the same
frame chain and the same frame. No d1 label, no B6 hypothesis and no clone or
injection change is supported by this.

Third, the host is not uniformly unable to run a guest. Inside job
`t7-bdad4cbc-b6-observation-r3` on 2026-09-06, the injection lane reached the
Windows kernel, its final `pc=0xfffff803dce999fc` resolving inside `ntkrnlmp`,
while the proof lane in the same job stalled at `0x1bf33ba04`. The stall is
confined to the configurations that boot the installed 68 GiB Windows disk.

What is not established is the cause. Every live queue job since 2026-09-06 has
failed, and the last live pass precedes the host reboot of 2026-09-02 19:35
local, so the failure window and the current host boot session coincide; that
is a correlation, not a mechanism. The host was not idle during these runs,
which is a recorded confounder, but a reference boot that reaches the desktop
in 26353 ms against a 120000 ms watchdog and a stall that produces zero of
eight milestones make host load alone an implausible sole explanation. No
macOS version or build change is recorded across the window.

No criterion is promoted or demoted by this record, no threshold or sample
count is changed, and no code fix is claimed. The practical consequence is that
the A11 freshness and no-regression gate cannot be re-established on this host
until this control passes again, and that the 2026-09-06 media-comparison
investigation ran entirely inside a window where the sealed reference
configuration itself does not boot. The next diagnostic changes one host-level
variable at a time and re-runs this exact manifest, beginning with a host
restart, since the host boot session is the only recorded difference between
the last passing and every failing observation.

## Candidate vtimer re-arm horizon refuted, 2026-09-07

The parked state was measured rather than inferred. In
`t15-e8ceebe1-host-boot-control-r1` the probe reports
`CNTVCT host=0x8cdfeb3a79e ... CVAL=0x8cdfe8ba7d6 gap=-2621384 ticks`, a
deadline 109 ms in the past, with `CNTV_CTL=0x1`, HVF vtimer mask false,
`GICR ISENABLER0=0x6c000000`, `ISPENDR0=0x0`, `ISACTIVER0=0x0`, `ICC PMR=0xf8`,
`IGRPEN1=0x1`, and the verdict `parked (deadline passed, wake still
deliverable)`. A deadline 109 ms stale at the end of a 120 s run cannot be one
the firmware armed and lost near the start; it is being rewritten
continuously, which on this path only `recover_swallowed_vtimer_fire` does,
once per canceled exit at roughly four per second.

That motivated a candidate: widen `REARM_FUTURE_TICKS` from 240 ticks, 10 us,
to 24000 ticks, 1 ms, so the re-armed deadline outlasts the host scheduling
gap before the thread re-enters `hv_vcpu_run`. It was built at
`02fcbd867210bf1a61b7ae44d7c12ca86d0a715f`, binary
`036b07f6daab7dcb08d28764db8339f7c63307cc03425e3735daa0c53f135394`, and run as
`t15-02fcbd86-rearm-candidate-r1` against the manifest whose baseline failed
three times earlier the same day, on the same host, with the same disk, vars,
renderer and firmware.

The candidate failed identically: no desktop, 0 of 8 milestones, a single
framebuffer checksum for the whole run, `pc=0x1bf33ba04`, and `CVAL` 2309523
ticks in the past. The 1 ms horizon changes nothing measurable, so the premise
that re-entry outran a 10 us deadline does not explain this stall. The change
was reverted; the tree keeps the horizon the earlier soaks were measured with
rather than an unproven one.

One further correction belongs in the record. The absence of `EXIT_VTIMER` was
not evidence of a swallowed fire. Every failing run reports `vtimer_exits=0`,
and so does the 2026-09-01 run that reached the desktop in 26353 ms; the
in-kernel GIC delivers the timer PPI without a userspace exit, so that counter
does not separate a healthy boot from this one. The reasoning that read a
swallowed fire from it is withdrawn.

What survives is narrower and still unexplained: the guest parks in the BDS
wait, the timer comparator shows an expired deadline with `ISTATUS` clear and
PPI 27 enabled but not pending, and no host-side re-arm at either horizon
dislodges it. The passing reference and the failing runs differ in host boot
session and host activity, not in any sealed input.

A procedural note for the next attempt. Committing any change under `crates/`
trips the registry freshness guard in `scripts/render-capability-status.py`,
which fails the project check with "code changed since tested_commit; re-prove
A11". The revert restored the guard because the file content matches
`tested_commit` again. A code candidate that is meant to survive must carry its
own A11 re-proof, not merely a green local check.

## Host-session hypothesis refuted; the stall is before the boot option, 2026-09-07

The previous entry proposed a host restart because the host boot session was
the only recorded difference between the passing reference and every failure.
That reasoning is now withdrawn: it was refuted without restarting anything.

Job `t7-bdad4cbc-host-contrast-r1` replayed the retained `t7-windows-closure`
manifest `cf1e062838fa386e1812be4b6fc5ec75c0cf1be75e0a3e1e42de2ab0f7e3ebc6` at
its original commit `bdad4cbc9426066bd4aed2045f08ad9e7dde74ba` on the current
host. Within that one job, minutes apart, the injection lane reached Windows
kernel space with final `pc=0xfffff802d562619c` and no PE owner within 512 MiB
below it, while the proof lane parked at `pc=0x1bf33ba04` with eight identical
splash frames. The same host, the same session and the same probe binary
`5912a1f2` both boot a Windows guest and fail to boot one. A host restart
cannot be justified by the evidence, and no host-wide inability to deliver
guest timer interrupts survives this contrast.

The device configuration does not separate the two either. The parked runs
include one with xHCI, virtio-net and virtio-gpu 3D all enabled and one with
all three disabled, and the 2026-09-01 reference booted with xHCI enabled. The
only factor common to every parked run and absent from the booting lane is that
the parked runs boot the installed 68 GiB Windows disk from its own variable
store, which is also the configuration that booted in 26353 ms on 2026-09-01.

Firmware serial output locates the stall precisely. The 2026-09-01 reference
prints `BdsDxe: starting Boot0002 "Windows Boot Manager" from
HD(1,GPT,E04E0289-B5EE-4D44-B90D-7D9FA5444B30,0x800,0x82000)/\EFI\Microsoft\Boot\bootmgfw.efi`.
The parked runs print the firmware banner and the console setup escape
sequences and then nothing at all. The boot option is never attempted. That is
consistent with the recorded frame chain, `DxeCore` RVA 0xdb44 and 0xdfd0 under
`BdsDxe` RVA 0xa3ac under `DxeCore` RVA 0xb820: BdsDxe has called into the DXE
core and is waiting there, after console setup and before
`EfiBootManagerBoot` announces a boot option.

The `Timeout` variable was checked and does not distinguish the stores: the
injector variable store and both installed-Windows stores carry the same entry.
That candidate explanation is closed.

So the search is not host-wide, not device-shape, and not a boot-option launch
failure. It is confined to whatever BdsDxe waits for between console setup and
starting Boot0002, on media that used to complete that wait in seconds. No
criterion, threshold or sample count changes, and no cause is claimed.

## Root cause: the vtimer recovery holds the in-kernel GIC down, 2026-09-07

The wait was named rather than inferred. `BdsDxe` was extracted from the
shipped firmware volume `edk2-aarch64-secure-code.fd`: outer FV at 0x1000,
file `9e21fd93-9c72-4c15-8c4b-e77f1db2d792` of type 0x0b, one LZMA
GUID-defined section, an inner DXE FV of 86 modules. The recovered PE reports
`AddressOfEntryPoint` 0x108bc, matching the `entry=0x108bc` the live frame
chain records, so the RVA convention is the same. At RVA 0xa3ac the code is

```
ldr x5,[x0,#0x50] ; gBS->CreateEvent, w0 = 0x80000000 EVT_TIMER
bl  #0x1084       ; ONE_SECOND
ldr x3,[x1,#0x58] ; gBS->SetTimer, w1 = 2 TimerPeriodic
ldr x3,[x0,#0x60] ; gBS->WaitForEvent, x0 = 2 events
blr x3            ; <- the parked call
ldr x1,[x0,#0x70] ; gBS->CloseEvent
```

which is `BdsWait` in `MdeModulePkg/Universal/BdsDxe/BdsEntry.c`, the boot
countdown waiting on a one-second periodic timer event and the hotkey event.
`ArmTimerDxe` in the same volume reads and writes only `cntv_ctl_el0`,
`cntv_cval_el0` and `cntvct_el0`, with no `cntp_*` access at all, so the DXE
tick has exactly one source: the virtual timer PPI.

`recover_swallowed_vtimer_fire` unmasked the HVF vtimer, pulsed that mask and
rewrote `CNTV_CVAL` on every canceled exit, roughly four times a second in a
parked boot. Under the in-kernel GIC created by `hv_gic_create`, which owns the
timer and delivers PPI 27 with no userspace exit, that is what keeps the
interrupt from ever going pending. `ArmTimerDxe` stops ticking, the one-second
event never signals, `BdsWait` never returns, and the firmware holds the boot
progress bar without ever starting the boot option. That is the whole chain
from the frozen frame to the register state: expired deadline, `ISTATUS` clear,
PPI 27 enabled and not pending, `vtimer_exits=0` in healthy and parked runs
alike because this path never used `EXIT_VTIMER`.

An interleaved A/B settled it on one host, alternating in time, with identical
disk `5ad7a304`, vars `2f0e6892`, renderer `dc596bf3` and firmware `b1dc201b`:

| job | role | binary | outcome | desktop |
| --- | --- | --- | --- | --- |
| `t15-e8ceebe1-host-boot-control-r1/r2/r3` | baseline | `df08c66f` | failed | none |
| `t15-ab-baseline-o1` | baseline | `df08c66f` | failed | none |
| `t15-ab-baseline-o4` | baseline | `df08c66f` | failed | none |
| `t15-23fecb43-norecovery-r2` | candidate | `37018b44` | completed | 21382 ms |
| `t15-ab-candidate-o3` | candidate | `37018b44` | completed | 30300 ms |
| `t15-13fb8213-fix-o6` | shipped head | `4385f381` | completed | 27284 ms |

Baseline 0/5, recovery disabled 2/2, and the shipped removal 1/1. The passing
runs print `BdsDxe: starting Boot0002 "Windows Boot Manager"` on the firmware
serial and record the `windows-boot-manager` and `edk2-bds` milestones, which
no parked run ever reached.

The recovery is removed rather than gated, because its own header already
recorded the same dynamic under the userspace GIC, where it was disabled for
exactly this reason: the re-arm outran the synthesizer and the cure blocked the
cure. No configuration is left in which it is measured to help. The 2026-08-06
soak that motivated it, a 21/21 correlation between surplus cancels and the A1
boot stall, is retained in the module header as retracted history; if that
stall returns, the fix is derived against current HVF rather than restored.

Scope of the damage. Every live queue job from 2026-09-06 onward was measured
under this defect. That includes the `t17` pilot, the three `t7` B6
observations, the `d1` media comparison and its published receipts, and the
`t18` B7 control. None of those results promotes or demotes anything, and the
d1 conclusion that the original and reinjected media fail identically says
nothing about the media: both were parked by the harness. The 2026-09-01 and
2026-09-02 passes predate the configuration that triggers it.

## The live gate system is restored: B7 10/10 at the fixed head, 2026-09-07

The fix was taken back through a real criterion gate rather than only the cheap
boot tier. Job `t18-13fb8213-b7-restore-r1` ran the sealed B7 audio profile at
code head `13fb82138158e23faf8c00e8d10d790f1a0804b8` with the same disk
`5ad7a304` and vars `2f0e6892` that the parked control used, differing from the
2026-09-02 seal only in the probe binary,
`4385f3817072ff94819a54ee002d18c5025779f67a2d17124789182232bbd974`.

It passed **10/10** in 521000 ms, against 568000 ms for the 2026-09-02 seal and
1555000 ms for the control that failed on lane 1. Input manifest SHA-256 is
`f379af78331f230981442a0270499d87e7a1e7ce615d5f2bf730eb90f0ee372c`, run-log-set
SHA-256 `8fa14fce24dacaf6b7f098f459fda500ebeeadf66b79b9ea8edb99b7cf738eec`, and
public receipt SHA-256
`ef0893405758d7f5eef9056810d01fec48d07f4346e835504ca9a9005895cafb`. Across
2,492,193 rendered frames it recorded zero drops, zero unexpected callback
errors and zero AudioQueue stop or dispose errors, with all 30 callback
statuses typed as expected stopping EnqueueDuringReset. That is the same
quality shape as the original seal.

One observation the gate does not measure. The host was running other
workloads, and a listener reported that the ten simultaneous lanes sounded
audibly choppy through the Mac output. The receipt records zero drops on the
guest-to-host PCM path, so this is host-side output contention across ten
concurrent AudioQueues rather than anything the criterion counts. It is
recorded here rather than dismissed, because "the counters are clean" and "it
sounded clean" are not the same claim.

With this, the queue is producing passing criterion evidence again for the
first time since 2026-09-02.

## Injector copy-verification reseal, 2026-09-07

The first B6 observation on the timer-fixed probe,
`t7-c193b7c0-fixed-b6-observation-r1`, failed honestly on
`firstboot stage4 readiness timeout` with the guest at the desktop and the
agent alive for the whole 2700 s wait. The retained prepared disk and the
sealed injector's `boot.wim` show the cause: `bvinject.cmd` verified the
package-local cleanup copy with `fc /b`, WinPE ships no `fc.exe`, and the 9009
took the block to `goto :end` before the pending flag and activation service
were planted. Full account, hashes and inspection method:
[b6-injector-firstboot-plant-failure-20260907.md](b6-injector-firstboot-plant-failure-20260907.md).

Code head `7f31bfc8d97ebe173823f156ba99b13e6085295b` replaces that check with
the copy's exit status, a `%~z` size comparison and the `find /c
"[CmdletBinding()]"` header check `bvgpu-firstboot.cmd` already applies, and
adds `tests/integration/hvf-windows-injector-winpe-commands-smoke.sh`, which
tokenises every command the injector invokes and fails on anything outside
cmd builtins plus the executables measured present in the sealed WinPE image.
The smoke fails the pre-fix injector on `fc` and two synthetic violations on
`certutil` and `powershell`. Full local `scripts/check-project.sh` PASS.
[CI 34157310286](https://github.com/Ketchio-dev/bridgevm/actions/runs/34157310286)
completed with every independent required job successful and only the
dependent capability and documentation drift job failing against the older
tested commit, as expected for a code head; [Security 34157310288](https://github.com/Ketchio-dev/bridgevm/actions/runs/34157310288)
succeeded. This section reseals `tested_commit` at that head and must itself
pass hosted CI and Security before it is a green seal.

A fresh injector was built from this head through
`scripts/build-hvf-windows-viogpu3d-injector.sh` with the same viogpu3d
package tree (`c46486e5…`), agent (`b7820834…`) and display-only marker as
the sealed 2026-08-29 injector; its SHA-256 is
`98ad6b3b23315a149ab38136e84bd14785da09aba50c9459aa74d28ae07ced01`. Job
`t7-7f31bfc8-fixed-injector-b6-observation-r1` (manifest `71631906…`) is the
first observation with it. B6, A9, B8 and B9 remain open and the product
remains Engineering Preview.

## Closure-tier shutdown reseal, 2026-09-07

The observation run recorded in
[b6-glyph-observation-active-iosurface-20260907.md](b6-glyph-observation-active-iosurface-20260907.md)
also exposed a tier defect: F4 types into Notepad, so `WM_CLOSE` raises the
save prompt, the window survives, and the tier's plain `shutdown /s /t 0` is
vetoed; the guest idled at the desktop for 50 minutes until the 3000 s watchdog
cancelled the run. Code head `64a38e83d4c256f9552b1442d43f08f5cbd6f7e2` forces
both closure shutdowns and, only after F3 has already been judged, runs
`scripts/win-assets/bv-windows-closure-discard.ps1`, a separate CRLF guest
script that ends the process owning the surviving window and reports
`BVDISCARD`. It is not a Coherence verb and cannot turn F3 into a pass;
`tests/integration/windows-closure-discard-smoke.sh` pins the forced
shutdowns on both paths, the ordering after the F3 verdict, the share and the
line endings, and fails the pre-fix script and two synthetic regressions. Full
local `scripts/check-project.sh` PASS.
[CI 34176195419](https://github.com/Ketchio-dev/bridgevm/actions/runs/34176195419)
completed with every independent required job successful and only the
dependent capability and documentation drift job failing against the older
tested commit; [Security 34176195432](https://github.com/Ketchio-dev/bridgevm/actions/runs/34176195432)
succeeded. This section reseals `tested_commit` at that head and must itself
pass hosted CI and Security before it is a green seal. The discard path is not
yet live-proven; the next closure run will show it. B6, A9, B8 and B9 remain
open and the product remains Engineering Preview.

## Legacy integer buffer sampler reseal, 2026-09-08

Tested code head `f1018e977718b409b3216d8d308a26222ad93716` corrects legacy
TGSI buffer sampler typing without changing explicitly declared SVIEW types.
The caption source and full diagnostic history are retained in
[the glyph observation record](b6-glyph-observation-active-iosurface-20260907.md).
The clean, non-instrumented candidate restored the classic Notepad caption
in live job `t7-caption-clean-20260908-r1`, receipt `3f161aa0…`; this is not
the required B6 resolution/scale and frame-time campaign.

Full local `scripts/check-project.sh` passed. Hosted
[CI 34186657230](https://github.com/Ketchio-dev/bridgevm/actions/runs/34186657230)
passed every independent job, including real native shader translation tests
for unsigned/signed/float inference and explicit declaration precedence. Only
the dependent capability/documentation job failed against the previous tested
commit. [Security 34186657180](https://github.com/Ketchio-dev/bridgevm/actions/runs/34186657180)
succeeded. This registry-only reseal must itself pass hosted workflows.
Product state stays Engineering Preview; B6, A9, B8 and B9 stay OPEN.

## F3 post-close confirmation reseal, 2026-09-08

Tested code head `5261db567505691093f4731c7e612c0e4fc4db03` adds a
diagnostic-only active-IOSurface capture between `WINCLOSE` and the guest-side
discard in the closure tier; it changes no pass/fail outcome. Full account,
including the retracted first submission at the wrong commit, is in
[the F3 confirmation record](f3-postclose-save-prompt-confirmed-20260908.md).
In short: the standard Win32 "save changes?" prompt is now directly captured
and OCR'd, not inferred, and the existing forced-shutdown-plus-discard path
already handles it.

Full local `scripts/check-project.sh` passed. Hosted
[CI 34187673040](https://github.com/Ketchio-dev/bridgevm/actions/runs/34187673040)
passed every independent job and failed only the expected stale
capability/documentation guard against the previous tested commit.
[Security 34187673048](https://github.com/Ketchio-dev/bridgevm/actions/runs/34187673048)
succeeded. This registry-only reseal must itself pass hosted workflows.
Product state stays Engineering Preview; B6, A9, B8 and B9 stay OPEN.

## B6 tab-scene spike reseal, 2026-09-08

Tested code head `9937b3120f1ce1469f36ca06eff20fa1dc410a78` adds a
diagnostic-only guest asset, `scripts/win-assets/bv-b6-explorer-launch.ps1`,
used only by an ad hoc, uncommitted harness variant to spike the B6 tab
scene question. Full account:
[the tab scene spike record](b6-tab-scene-spike-20260908.md). The asset
itself is not wired into any shipped closure gate.

Full local `scripts/check-project.sh` passed. This registry-only reseal must
itself pass hosted workflows. Product state stays Engineering Preview; B6,
A9, B8 and B9 stay OPEN.
