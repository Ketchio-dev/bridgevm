# Archive isolation and native UI diagnostics — 2026-10-14

Evidence ranks: **automated tests** and one **failed live diagnostic**. This successor work
starts from `3ea7d1a7c3413712d4fdbc25d60052e94e5b100f`. It preserves the failed earlier
candidate and adds reviewed changes; it does not convert earlier failures into passes or close a
release criterion. Counts below describe separate, overlapping proof scopes, not a unique total.

## Earlier candidates remain distinct

Exact source `c2bd3f2a5f4d2d56ecec548c26d15e0bf3e6fbca` finished its 109 associated hosted
workflow runs with **108 successes and one failure**. Manual CI run `38628657395`, Cargo macOS 15
job `116074274829`, failed `concurrent_public_tar_exports_preserve_exactly_one_winner`: the tar
reader encountered an invalid/nonnumeric checksum field while reading JSON as a header. That
storage unit run recorded 116 passes and one failure. The raw failure is retained; another
passing CI venue does not override it.

The separate local project check at exact `3ea7d1a7` passed all 45 stages, 20:34:55–20:56:14
UTC. Before/after commit, tree, index, tracked-source digest and clean status matched. This
local result does not erase the inherited hosted failure or establish full verification for
subsequent changes.

## Concurrent tar export uses an exclusively owned stage

Integrated commit `fdedadada067e7eb92eaf0e41edb4801601017c9` reserves a private directory before copying.
Timestamp/PID names are candidates; exclusive directory creation establishes ownership before
any cleanup guard is constructed. An occupied file, directory or dangling link is preserved and
another candidate is tried, with a finite reservation limit and normal errors retained. The
existing exclusive destination-file creation and archive layout remain.

The unchanged public concurrency test failed locally: the declared winner's archive contained
another source's disk payload. A diagnostic-only series failed 39 of 104 runs; all 39 failures
shared a staging pathname. Retained archives demonstrated mixed source metadata and payload,
while original synthetic source bytes remained intact. The exact hosted checksum symptom was
**not** reproduced locally; no claim equates the two observed symptoms. Temporary path logging
and failure-retention instrumentation were removed.

After repair, the unchanged public test passed 100 of 100 finite repetitions. The storage suite
passed 120 unit and eight integration tests (128 tests), including reservation controls; clippy,
formatting, budgets and diff checks passed. The first fixed build's test-helper visibility
failure is retained. These checks do not prove all schedules, a snapshot against external source
mutation, hostile pathname replacement, crash recovery or atomic publication. The existing
partial-output and pathname-based cleanup limits remain.

## Concurrent tar import isolates outer extraction

Integrated commit `85ffb1ccab4000f5e316225bc8d193b927c23602` addresses concurrent public imports into independent
stores. At unchanged c2, the serial control passed and the concurrent method
failed with busy/missing-manifest errors. Diagnostic pathname observation showed two distinct
tar inputs choosing the same outer extraction path. Successful mixed-content publication was not
reproduced.

The repair exclusively reserves a private parent before constructing its cleanup guard, then
extracts into a bundle child. It preserves the original tar in metadata.source, inner
transaction admission, source verification and no-replace publication. The unchanged two public
methods and four reservation/extraction-cleanup controls passed. Twenty finite repetitions, each
with eight rounds of four imports, passed: 640 operations, not 640 unique tests. The storage
suite passed 123 unit and eight integration tests; clippy, formatting and budgets passed.
Diagnostic path logging was removed. These results do not establish hostile namespace
protection, arbitrary permission-failure cleanup or every interleaving.

## Separately versioned accepted-cancel/retry coverage

Integrated commit `c9a0138042c8de40c026212213cf3e6af9e4bdee` adds the diagnostic tier
`d6-app-ui-install-cancel-v1`. This addresses a coverage gap, not a reproduced product defect.
Existing overview v1/v2 schemas and fixed action/capture counts remain unchanged. The new
scenario has exactly three AX mutations: Start, Cancel while validation is accepted, then Retry
after cancellation.

The real detail/preparation/install-controls route uses one preselected synthetic pending VM.
Its first validator waits on a finite off-main gate; after actual AX cancellation readback the
gate is released and cancellation settles. Retry returns a fixed validation error. Injected
process, backend, scheduler and runtime paths refuse effects even if validation is admitted.
Four tiny owned configuration/helper-marker files are checked for exact inventory, ownership,
single-link file checks and unchanged hashes. No installer helper or guest starts. No disk, ISO
payload, vars or keys exist.

Five phase observations bind session identity, request sequence and actual AX-driver results.
Their captures use AppKit cached bitmaps, not independent WindowServer pixels. Independent
host/driver identity and exit verification remain mandatory. The result explicitly disallows
capability promotion.

Focused native XCTest passed nine methods: two new session fixtures and seven existing host
controls. The same two new test files passed the actual repository shim, with zero
failures/skips; the seven old controls are native-only in this focused proof. New pure protocol
and injected-AX checks passed 95 and 69 checks; existing protocol/AX checks passed 91 and 39.
The driver entrypoint also passed ownership controls, five packaging tests, 15 old-v2 Python
tests and 18 new Python tests. These counts overlap.

Review exposed five verifier acceptance gaps, each preserved as an unchanged failing regression:
terminal extra read, equal-byte hardlink, optional null, deadline drift and out-of-range PID.
The repaired new Python suite passed 18 tests; a final replay covered the last finite-positive
launch-date guard. Native/shim compile-input failures, a readonly shell-variable wrapper error,
and two shellcheck required-variable findings remain in the record. A passing driver attempt
omitted explicit TMPDIR; later authoritative driver runs used owned scratch. Existing native
host fixtures retain their prior temporary-directory APIs; their individual locations were not
re-proven. Both independent Swift/UI and root Python/queue reviews were clear. No physical
action or ordinary asynchronous-startup claim follows from these unit, injected-AX, protocol or
packaging checks. This new tier has not run live.

## The existing overview-v2 live diagnostic failed

Job `dev24-c2-native-ui-v2-20261014-01` ran the unchanged c2 overview-v2 source,
20:53:36–20:53:53 UTC: **17 seconds, FAILED**. It was explicitly a development diagnostic while
that source still had a hosted failure. The driver reported accessibility untrusted before any
request or mutation: zero AX requests, zero mutations and zero completed UI actions. No
permission change or automatic retry was made.

Both requested dark rows failed with original content Aqua at the correct sizes. The first
dark/default failure sidecar additionally records application DarkAqua and window Aqua. These
mismatches preceded the final accessibility error and remain in separate matrix/failure records.
Two light AppKit captures exist; there is no dark image or successful dark-presentation result from this run.
Static review found ordinary ContentView's unconditional light preference conflicted with the
requested dark matrix. It did not establish the exact framework writer or prove a repaired
appearance.

Both owned application identities, observed exits and cleanup were verified; neither required a
termination signal. File-job, install, control-model and runtime tripwires all remained zero.
Cleanup success is not UI success. The new install-cancel diagnostic was absent from the tested
c2 source. No guest behavior, Windows installation, release gate or criterion was tested.

## Ordinary appearance follows the inherited theme

Integrated commit `617691b9eea51394a2d24467982dd0a99d2088c1` removes the unconditional
`.preferredColorScheme(.light)` from ordinary ContentView in both release and DEBUG builds.
The semantic palette, actions and diagnostic presentation requirements remain unchanged;
ContentView's structural ceiling falls from 284 to 283 lines.

Seven existing native host methods passed, and the ordinary release compiled without the host
identity marker. These checks do not establish a rendered dark-mode repair or contrast result.
No corrected-source app, GUI or AX run occurred in this lane. The earlier failed physical
observation remains failed; a later combined candidate would not be an isolated single-change
comparison unless separately demonstrated.

## The release-refusal checker recognizes both exact diagnostic layouts

The original boundary invocation failed even though the forbidden release correctly reached its
compiler refusal: the checker did not recognize the swiftbuild diagnostic layout. That failure
remains recorded. Integrated commit `513e34911ab387676e513e039760141262c99ef4` replaces the suffix search with two exact
diagnostic-line layouts, tied to the expected source guard and location and a nonzero compiler
status. Warning/source echoes, altered text/path/location and
successful compiler exits remain rejected. Seven parser regression methods passed. Native-layout
provenance is an exact guard-fragment typecheck, not a native-build-system full app compilation.

The corrected full boundary replay passed at 21:10:12–21:10:46 UTC: positive diagnostic marker,
ordinary release marker exclusion, and exact forbidden-release refusal all remained required.
It used the existing Swift 6.4 default swiftbuild path and executed no application artifact.
Source-identical seven-method native evidence was reused after the parser-only merge. The
existing checker ceiling stays 53 lines; the new helper and tests are registered at 34 and 88.

## Diagnostic packaging can select an exact certificate

Integrated commit `5ed3b003128aa830a840530aed15f1526f02c0bd` adds optional `--signing-identity` with an exact
40-hex certificate fingerprint. Omission preserves ad-hoc
signing and the existing four-argument packaging API; no environment identity fallback is added.
The selected route uses absolute codesign with a 20-second timeout per call, strict complete-bundle
verification and an exact leaf-certificate requirement. Successful command results are recorded
without inferring signer class or trust. Failure stops without retry/fallback and retains partial
output without a success receipt. Original inputs remain checked; the bare supervisor is copied
from the signed driver and its bytes are compared.

The focused selection passed 48 tests: five existing actual packaging controls, nine new mocked
controls, one real ad-hoc requirement-refusal control, 15 overview-v2 and 18 install-cancel tests.
The original option-rejection baseline and a later real parser failure are retained. That parser
failure exposed a missing inline `=` prefix: requirement text was treated as a filename. After
correction, the real ad-hoc control proved valid requirement syntax and certificate refusal;
it did not exercise a certificate-bearing positive signature. No private-key signing, TCC change,
actual Accessibility trust, stable designated requirement or release qualification is established.

Publication correction: `5a846f8aeff5d5bb928d563eccbab430a2812e04` failed hosted documentation
references in run `38639077520`/job `116109688818`: six local author commits were unavailable.
The local checker had those objects and missed this defect. This document now cites reachable
integration commits; retained external receipts preserve author mappings. The same six failures
were reproduced with public GitHub refs only. No reference-check rule was relaxed.

## Evidence bindings and remaining limits

The following SHA-256 values bind retained reports and raw evidence. Local paths, fixture
contents and private machine details are intentionally omitted.

| Evidence | SHA-256 |
| --- | --- |
| c2 hosted terminal 109-workflow inventory | `a5e3a8f5e1d6dbd43c501fcf6cfd436765ffe0c541dd8ff43d1cb4e973810eb1` |
| c2 failed Cargo raw log | `49938828fab94645620e5adf916c797c47f5f10625f9272235da9dbbe67a0590` |
| 3ea local 45-stage log | `8feeacc8642994407b14641d65ef33a644f2c3596c23941d0e3f55f265c08e97` |
| Export repair report | `a37a070f1eb929807e010e3673a5d085e495b99244abd5f971a19ef4d5b7a4a9` |
| Export independent review | `f2e87f92fb7ce02eb1ceee4a81df620040257a60a8c52bdfcf7689ad2c0b8b8c` |
| Import repair report | `e3f053e2c8d7d506ae4c967edb5de084c4fe1580ec288be40f0f9ec5cee38d10` |
| Import independent review | `eccd91f7680df54fdd64d4288086c55ed1b3098a6cff0f7bb3ec30bcd8be2f12` |
| Import repair patch | `19d61c3dc407a24ff0fbc7f41a347e22fd23597d2ec1f365fe6cc5dfc8c05425` |
| Install-cancel source/proof report | `cb2dd88834f59745d3dfe7582c7906234298e91ac0a5d06b04ceaf36c6f60adf` |
| Install-cancel independent Swift/UI review | `568067f6037672b6a0ed7bb8da3068baf3be75f26aba678c384fef8cb48d6535` |
| Install-cancel root Python/queue review | `a13a844c7760825226af03d291e411f4f3538511a2465a77b7234a744dad5b90` |
| Actual overview-v2 result audit | `caea8aff43023b0a15dc6d2d3ff6c5a3f47dffa9aebf89650af181598412cae7` |
| Independent presentation-failure review | `a7101a3ae315326ddcdd59b452d0d5ce8d4db229e0032d74f499cc352a963bb2` |
| Appearance and boundary final receipt | `7f00369fc4a00d8b38e3be367d8c36f991982ac7aa84903a4ab2f20a41cc9b1e` |
| Ordinary appearance independent review | `b2b57ce2fa5c923ee4ed85c7f64e7eee4fde945b6fb25acef0ee4991905c1888` |
| Appearance and boundary source inventory | `751adfd440526f7939354e7324bdeb4444b09375032142356e26e3c98b7c78e6` |
| Exact-layout parser focused proof | `d58f75d8ca748bc68783b6bd6a1c53f854f8f38df987d1b7329bcb1b2da69102` |
| Exact-layout parser independent review | `8f9d81364447c2277277bc5b4e846d7a2182ee8066d45ae6fa18b0ff608117ff` |
| Optional signing source inventory | `9022e0be8291ccd54548c01f02c8a827f178d2dc8ad942ed660e485b341dd5f9` |
| Optional signing patch | `c757f5c51083cda0926349788f7754e38d60555724cf7efb185622b2642f0f88` |
| Optional signing focused receipt | `88e332ee103f7581440348ecbd5aaec08709bdaa0665210e91221872a63a1513` |
| Optional signing independent review | `e8c7a46602bd4f1c6be83348c1a6c98b24f513f3e5a368279faa1657ae44dc92` |

Successor full local and exact-SHA hosted checks remain required. Existing structural ceilings
were retained or lowered; new files use measured budgets. Product state, capability wording,
criterion thresholds and known defects remain unchanged. A9, A11 and A19 remain OPEN; no live
criterion is closed.
