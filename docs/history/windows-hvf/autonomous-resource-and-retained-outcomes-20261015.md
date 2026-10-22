# Saved resource display and retained outcomes — 2026-10-15

This follows the [package failure and move-selection record](autonomous-package-and-selection-20261015.md).
It records automated checks and source review. The tenth candidate requires its own
complete local and exact hosted checks. Product state, capability wording, thresholds
and known defects remain unchanged.

## Ninth failed run and separate controlled run

Ninth `d1171eda29b34b78b7273fa8595e6c0b00808eb6` first completed its local full check
at 02:02:21 UTC on 2026-10-15: 43 PASS / 2 FAIL across 45 stages, exit 1.
The failed stages were Windows product/A19 contracts and install verification.
Two queue-fixture assertions failed and one B8 tar round-trip method errored.
All 5,132 tracked files, HEAD, tree and index remained unchanged.

Later root-agent attestation identified inherited `umask 077`; it was not a raw capture
of the original launch environment. Independent unchanged-source comparisons repeated
the same three methods under each mask: they failed under `077` and passed under `022`.
The queue fixture's requested mode 0755 became 0700 under `077`, changing its intended
nonprivate-alias setup. Native tar extraction likewise changed the tiny executable's
mode from 0755 to 0700 while retaining its bytes; the exact-mode verifier refused it.
No assertion, security check or acceptance threshold was relaxed. These observations
do not establish complete-installer behavior or arbitrary-umask robustness.

A separately retained invocation explicitly set `umask 022` and used fresh owned scratch.
It completed at 02:20:21 UTC in 989.861 seconds: all 45 stages passed, exit 0, on the
same exact source. Its shim suites passed 425, 1,427, 65 and 379 methods, with no failures
and three existing Control skips. These are suite results, not added unique coverage
across native and shim venues. The original 43/2 failure remains failed and preserved.
The separate ninth hosted capture subsequently recorded all 103 original exact-head workflows
successful: 47 push, 53 pull-request and three manual runs, all on their first attempts.
Its 172 jobs comprise 170 successes and two optional/advisory skips. The
[hosted full check](https://github.com/Ketchio-dev/bridgevm/actions/runs/38667925161)
passed all 44 executed stages; the unavailable optional Linux target was skipped.
The full log authenticates checkout `d1171eda29b34b78b7273fa8595e6c0b00808eb6`;
eight actual job checkout samples were retained, including a distinct PR merge with
the same tree. This does not establish every job's checkout from API association alone.
The final independent bundle review found no blocker, with a retained nonblocking
qualification: supporting raw-hit lists included pre-checkout lookups under checkout labels;
the final report already selected actual HEAD evidence. No counts or frozen records changed.
Hosted success does not rewrite either earlier failure.

## Successful Apply publishes the saved resource values

Integrated `b57f23f11d3c3b291dabe8b3fa246509c5d1a3c2` corrects the resource display
at the successful save acknowledgment. An actual FastVZ allocation write could save
8 GiB / 4 CPUs while published model fields still showed 4 GiB / 2 CPUs. A read admitted
while Apply was pending could also publish the old values after the save completed.

The model now publishes committed memory and CPU values beside its saved configuration
before the existing persistence callback. It also invalidates earlier refresh generations
at that boundary, using the existing stale-result guard. Draft edits made while the save
was pending remain intact. A failed write retains the prior configuration, displayed
values and file bytes together with the requested draft. Admission, resource bounds,
status text and the existing stop/restart flow are unchanged.

The initial four-method baseline had two failing methods and four assertions. The
extended five-method baseline had three failing methods and nine assertions; its first
four cases overlap the initial run. Final native and actual repository shim runs each
passed the same 23 methods: five new cases and 18 related controls. They are 23 unique
methods across those venues, not 46. The shim had no skips. No compile failure occurred;
configured-mode logs retain test-support semaphore warnings about strict Swift 6 mode.

The UI route was checked statically; tests exercised the model, real allocation writer
and generation boundary with tiny owned JSON and inert process methods. A supplementary
100 ms absence-of-old-emission observation does not independently prove that the refresh
task completed. No rendered window, physical click, second Apply, VM or guest was tested.
Unchanged validation/conversion code moved into a 15-line helper; the model remains within
its 135-line ceiling and five new files have budgets at their actual sizes.

## Dismissing a terminal retained control restores the current registration

Integrated `de5d9de749be7573337039c1a097f795c9490779` fixes the next Start action
when a registration changes under an active old control and that retained control later
becomes terminal. Dismissal returned to the current registration's dashboard, but its
cache could still return the old retained object. Its unchanged Start guard then refused.

The lookup now constructs the current registration's ordinary model when the cached object
is retained and has no active runtime work. The old retained object remains unable to
relaunch. An active running handle, an already-current cached model and an ordinary idle
model retain their identities. Pending, start-confirmation and owned-cleanup preservation
follows the unchanged active-work predicate by source review; those phases were not newly
exercised as runtime controls. Registration persistence and dismissal/selection logic stay unchanged.

An initial fixture compile failure executed zero methods and remains in the record.
After correcting that fixture, the actual five-method baseline passed three methods and
failed two with eight assertions. Final native and actual shim runs each passed the same
11 methods: five new cases and six related controls, not 22 unique cases; the shim skipped none.
The cases follow actual persistence/reload, sidebar selection, retained dismissal and the
ordinary dashboard Start closure. Running/terminal state is synthetic and process methods
are inert fakes. No rendered window, physical AX dispatch, real process, helper, VM or guest
was exercised. The three new file budgets are their actual 39, 92 and 80 lines; no ceiling rose.
Independent patch/commit and combined implementation reviews found no blocker.

## Remaining validation boundary

The tenth candidate still requires its own full and exact hosted checks. Separately, the
seventh native-UI single observation on `d3cace7877ae7c0e1781d36e1ae77ed8e7eb6040`
ran and failed despite the driver's reported Accessibility trust. Its one `welcomeControls`
request returned AX error -25201 at `copyAttributeValues` / `AXChildren` / `element`.
It recorded zero completed samples and zero boots, with cleanup recorded true. The failure
reader attributes the API/context only; the underlying OS cause remains unknown and it does
not independently reverify cleanup. Independent review of the actual run remains pending.
Neither this failed observation nor deterministic checks promotes a live criterion,
product state or release conclusion.

## Evidence bindings

The table lists hashes only; retained external records may contain private paths.

| Record | SHA-256 |
| --- | --- |
| Ninth original local full result | `ae207cffe2c0edabec95ffb3f167e9da2255d5a26237c6cb6b7df24f4a8472ea` |
| Ninth original failed-full monitoring receipt | `9b6865dedbb727cabd39d3449e874858346ddf4800b32beac0912503ade2c66c` |
| Independent unchanged-source umask diagnosis | `0f360c9dc14f42aeafd3d8d3b42741c89e74623c8f09d00badbf96bc3ea03d92` |
| Ninth separate 022 local full result | `196dbecfa97a64679eb8c5cd239effd1d4fcd30f6fc48c49b54c9492251ac0f9` |
| Ninth separate 022 monitoring receipt | `f03da9c48b36ccc026b1043372b344bd1090e0551dd4c3554f7b670b6cdcdb33` |
| Resource display source/proof report | `ba59dc1e7ede040e1187d7d8b7d87d7fc216bb0908824eaf104faaccbcbb0c28` |
| Resource display independent commit binding | `f18f1c600f7e67ec29248e4f745e650e915e0283c53b8144ac5db1b2a6539743` |
| Ninth hosted terminal capture | `107742b36b1863eabe1e0fd5c4217e36158e69c503bee2764dc64c671fb163b8` |
| Retained-terminal source/proof report | `f90162fb3d1ef8f9b56ba897863eb90f1c0b65909486a9b45f58a00ad48151b8` |
| Retained-terminal independent commit binding | `78c1a690760c04f6cc6c0b1407de2e6c977e06461ff91d0fef80be10b08e5d2d` |
| Tenth combined implementation review | `7209aa82a91ca547636dc8ee92dc3dad9b24c7a8715240222ec97a2967a122e5` |
| Ninth hosted independent review | `743cff8877d906440c1f71cf18757e946c5d8fda373e418a12b8749670020832` |
| Seventh failed native-UI callsite audit | `1716401eae28c4121385accda85698c907f885035b77ca84be0dc2241a6a860d` |
