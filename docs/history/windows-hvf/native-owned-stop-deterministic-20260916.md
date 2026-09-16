# Native app-owned stop — 2026-09-16

Classification: historical deterministic evidence. Current product state belongs
to the [registry](../../../capabilities/windows-hvf.json). A11 remains OPEN and
the product remains ENGINEERING_PREVIEW. This record follows the
[runner cancellation and B6 fixture record](native-owned-cancellation-deterministic-20260916.md).

## Tested source

Source commit: `0d1fcbf6bf373404ed1454663d26ce590d7495b1`.
Tested tree: `004b97535df6e0c998fbc1de23bd0ba039f3ff7d`. The full project check ran against
3,120 frozen tracked inputs while HEAD was `3fe423b08fea40f2968c02dc876f60b42fcf9612`.
Its source hashes, index and HEAD remained unchanged. The source commit has
exactly that tested tree; no new test run at the committed SHA is implied.

The ordinary typed app launch now retains a framed runner channel and exact
process identity. The runner reports each helper generation and TPM child from
spawn through actual reap, and drops admitted media leases before completion.
The app requires validated completion, retained runner exit, clean output EOF
and drained guest command work before confirming teardown. Corrupt or missing
proof retains ownership and blocks replacement or destructive library work.
A late valid cleanup can release ownership without rewriting a missed deadline
as a successful operation. Guest shutdown and application flush remain unproven.

The private running-app endpoint retains read-only status v1 and adds strict v2
stop control. The CLI selects one exact app-owned target, waits for its durable
operation and preserves that identity across retries. It neither launches an app
nor controls attached/legacy processes. The outer Rust command forwards through
its installed pairing to the native executable. CLI start remains unimplemented.
The [user guide](../../app-cli-stop.md) and
[protocol reference](../../reference/native-owned-runtime.md) state the bounds.

## Checks and scope

| Check | Observation | Evidence limit |
| --- | --- | --- |
| First frozen full project check | FAIL, 174.61 seconds | New tests used XCTest APIs absent from the required shim |
| Corrected shim run | FAIL, 64.653 seconds; control suite 654 passed, 1 failed | Existing duplicate-launch diagnostic was accidentally omitted |
| Final frozen full project check | PASS, 219.13 seconds | All required deterministic project steps; no live guest gate |
| Final shim control suite | 657 passed, 0 failed, 2 skipped | Explicit WindowServer and private live-disk checks remain skipped |
| Focused runtime/runner families | 105 passed | One ignored test-only ECHILD entry is invoked by its parent test |
| Focused app/CLI tests before final corrections | 74 passed | Golden payloads, deadlines, retention, validation and reentrancy |
| Focused channel correction | 20 passed | Same assertions and three-second fixture bounds under compatible waiting |
| Restored attachment diagnostic | 18 passed | Original path checks plus publication/admission reentrancy |
| Standalone private socket contracts | 17 passed; 29 inputs unchanged | Foundation/socket checks including retained four-slot limits |
| Independent ordinary app/runner/CLI coupling | 3 passed; 2,587 code inputs unchanged | Actual binaries and synthetic files/fake children; no App.main or guest |

The independent coupling checks ordinary typed launch and app stop, mutation of
the session's later control path, atomic replacement of the original control
file, and actual paired Rust CLI stop followed by native CLI retry of the same
operation. The replacement stays untouched; cancellation reports runner exit one
with confirmed cleanup. Each fixture registers kernel exit witnesses while its
nonce-identified children are alive and removes its files only after observing
the retained runner and both children exit. These cases also run in the final
required shim suite. They do not exercise real Windows shutdown.

The runtime family also includes existing tests using an installed real swtpm
with newly generated synthetic state and keys. Those are not fake-child tests,
but they use no private VM media or operational TPM state. No guest disk, vars,
TPM state, key, or title content is included in this record or its hashes.

## Failures and review corrections retained

Rust setup failures, a marker publication race and a stale flattened argument
test remain in the runtime lane logs. A closed reader initially went unnoticed
by the idle Darwin output poll; an independent pipe probe established the
POLLOUT reporting difference. The corrected reactor detects the broken owner
without abandoning its children. Clippy rejected a redundant non-Drop drop;
removing it changed no lifetime behavior, and the corrected lane passed Clippy.

Swift failures include an inaccessible synthesized initializer, test fixture
canonical-order mistakes, a deliberately required runner environment missing
from one broad filtered run, and a blocked test-peer Foundation read. The last
was interrupted and replaced by bounded nonblocking test-peer reads. All logs
remain; these failed attempts are not counted as passing validation.

Independent review found and corrected final-proof deadline bypass, queued guest
effect release, STOP EPIPE discarding buffered completion, missing post-I/O/drain
deadline checks, ACK/COMPLETE identity mismatch, pending-response rejection after
runner exit, failed-cleanup response acceptance, and synchronous state/admission
reentrancy. The full check then exposed shim compatibility and the lost original
diagnostic; both were fixed without relaxing existing assertions or ceilings.

## Receipt identities

Private development records retain the following files. The first independent
coupling receipt mislabeled its end-time field as started_utc; the adjacent
correction records that label error without changing results or source hashes.

| Record | SHA256 |
| --- | --- |
| packet47b-source-seal.json | `2225c5993578e9678e0dc45d1cd47bc6f19b9569c135e15fa95fd3f80915b1e1` |
| packet47b-full-project-r1.log | `b01eb306b6ccf809a0a2c2ebbd31e3997a9b6c5ad7d3e07e8137c9f09aa24b72` |
| packet47b-shim-r2.log | `0cdf0cbf07c040cb3ee0e9e6beaddd860be3f924af505208ddb9a272b5167b8e` |
| packet47b-full-project-r2.log | `3166180a2dabb205331b9c2c0da5cdb965cd1bc290c951d09ffdebdec5f9cebc` |
| packet47-owned-runtime/source-ready-r2.json | `1ad1be19a89ea6eba794a67411c5f78b7c39fce3327d5e0797039e3d4afddb26` |
| packet47-swift-owned-stop/source-ready-r1.json | `c1c7b33e7f196647b518a8358baa481b2df12257ac6b1c9e4df5d2fabac72a15` |
| packet47-swift-owned-stop/shim-compat-source-r1.json | `77a334ecc19bac2242c78052debc5b65c5b06f1642408a7bcfdb7581c6597f7c` |
| packet47-swift-owned-stop/attachment-diagnostic-source-r1.json | `6b018c4fc5b3653250c5e7e988953467fab0f7b3d0771e1a76a54d94a88c3772` |
| packet47-native-control-transport/standalone-r3.json | `738e0a1a307eb739d48a8e6f939ef7638c0e893bb149c9d72b3cb5281dee3a2b` |
| packet47-root-owned-coupling/r1.json | `42353526af1209921df04baf09306e5ae5216aa8fb0a66196658cc21408a3156` |
| packet47-root-owned-coupling/r1.log | `a16ea2565c0d665a0e725d1a2723a7ab00e65ad0c810f71284e51dd6abd63deb` |
| packet47-root-owned-coupling/r1-timestamp-correction.json | `1c7547d7996c43fd4d52ede4ed0008ad6f4ac89dfe147190642694ecbb3fe690` |

The independent coupling artifacts were the actual host debug runner, paired Rust
CLI and native executable. Their hashes are retained in its receipt; later full
checks rebuild native artifacts as needed, so those earlier binary hashes are
not substituted for the final source seal.

## Hosted and live boundaries

PR159 was merged after all 85 exact-head workflows passed. Its post-merge checks
were still pending at the last bounded inspection. PR160 was updated with main
at source-identical head 1eb6a81e8281dbb0256c683eaea579f590e07f7c; its new-head
hosted checks and the revised B6 Windows checks remain unverified here. The
03:49:20–03:52:20 UTC hosted inspection window is closed. This new stop source
has no claimed post-push hosted result in this record.

Native UI pilot18 remains 0/7 actions and 4/8 captures with Accessibility
untrusted. No permission query, UI launch or physical live job was retried.
Live app/guest stop, Windows application flush, supervisor crash/SIGKILL recovery,
CLI start and release gate sample counts remain unproven. All 29 criterion
policy definitions, states and thresholds are unchanged.
