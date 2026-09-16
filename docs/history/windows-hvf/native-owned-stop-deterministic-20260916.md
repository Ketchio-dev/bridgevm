# Native app-owned stop — 2026-09-16

Classification: historical deterministic evidence. Current product state belongs
to the [registry](../../../capabilities/windows-hvf.json). A11 remains OPEN and
the product remains ENGINEERING_PREVIEW. This record follows the
[runner cancellation and B6 fixture record](native-owned-cancellation-deterministic-20260916.md).

## Tested source

Runtime source commit: `0d1fcbf6bf373404ed1454663d26ce590d7495b1`.
R2 tested index tree equals that commit's `^{tree}`; its full identity is in the receipt.
R2 checked 3,120 frozen inputs at HEAD `3fe423b08fea40f2968c02dc876f60b42fcf9612`.
Hashes, index and HEAD stayed fixed; no additional run at the source commit is implied.
Registration checkpoint: `553ec2a93e070c1e8dfb925bf6cd287edd191b55`.
R4 passed with 3,121 frozen inputs; hashes, index and HEAD stayed fixed.

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
| Frozen full project R2 | PASS, 219.13 seconds | Before later document budget registration; no live guest gate |
| Post-commit freshness | FAIL at `553ec2a9` | Budget TSV changed since tested_commit; re-proof required |
| Frozen full project R3 | FAIL, 211.033 seconds | Only document reference failed; 3,121 inputs stayed fixed |
| Corrected full project R4 | PASS, 210.805 seconds | Includes registration and corrected document reference |
| R4 shim control suite | 657 passed, 0 failed, 2 skipped | Explicit WindowServer and private live-disk checks remain skipped |
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
nonce-identified children are alive and removes files after runner and child exits.
These cases also run in the required shim suite; they do not exercise real Windows shutdown.

The runtime family also tests an installed real swtpm with fresh synthetic state
and keys, without private VM media or operational TPM state. No guest disk, vars,
TPM state, key, or title content is included in this record or its hashes.

## Failures and review corrections retained

Rust logs retain setup failures, a marker publication race and a stale argument test.
An independent pipe probe exposed idle Darwin polling missing a closed reader;
the corrected reactor detects it without abandoning children. Removing the
redundant non-Drop drop preserved lifetimes and resolved the Clippy rejection.

Swift logs retain initializer access, canonical-order and missing runner-environment
failures. A blocked test-peer Foundation read was interrupted and replaced with
bounded nonblocking reads. Failed attempts remain excluded from passing validation.

Review fixes cover final-proof deadlines, queued guest effects, STOP EPIPE drain,
post-I/O deadlines, ACK/COMPLETE identity, pending exit responses, failed-cleanup
acceptance and synchronous reentrancy. Shim and diagnostic failures were fixed
without relaxing assertions or ceilings.

Post-commit freshness failed because the new budget registration was absent from R2.
The prior `code_changes_after_full_check:false` statement was too broad: runtime
bytes stayed fixed, but a code-classified input changed. R3 then rejected a tree
object as a commit reference; the record now uses the explicit source-tree expression.

## Receipt identities

Private records retain these files. The first coupling receipt's started_utc was
an end time; its correction preserves results and source hashes.

| Record | SHA256 |
| --- | --- |
| packet47b-source-seal.json | `2225c5993578e9678e0dc45d1cd47bc6f19b9569c135e15fa95fd3f80915b1e1` |
| packet47b-full-project-r1.log | `b01eb306b6ccf809a0a2c2ebbd31e3997a9b6c5ad7d3e07e8137c9f09aa24b72` |
| packet47b-shim-r2.log | `0cdf0cbf07c040cb3ee0e9e6beaddd860be3f924af505208ddb9a272b5167b8e` |
| packet47b-full-project-r2.log | `3166180a2dabb205331b9c2c0da5cdb965cd1bc290c951d09ffdebdec5f9cebc` |
| packet47b-postcommit-freshness-failure.json | `db4efdb8aa0891a29a2637df4c228ca37ef5a5ca39c3d4c285ae07dfd0164d0b` |
| packet47b-full-project-r3.log | `8cfa5a811c7e28a0a649ee0b3a0d05e4edf6c2840cf915cf88cae37f3581b8fd` |
| packet47b-full-project-r4.log | `05d1798a1b7807e4b6db647f83573098b8bf1c7c880c006e2d1ad3c7a2005447` |
| packet47-owned-runtime/source-ready-r2.json | `1ad1be19a89ea6eba794a67411c5f78b7c39fce3327d5e0797039e3d4afddb26` |
| packet47-swift-owned-stop/source-ready-r1.json | `c1c7b33e7f196647b518a8358baa481b2df12257ac6b1c9e4df5d2fabac72a15` |
| packet47-swift-owned-stop/shim-compat-source-r1.json | `77a334ecc19bac2242c78052debc5b65c5b06f1642408a7bcfdb7581c6597f7c` |
| packet47-swift-owned-stop/attachment-diagnostic-source-r1.json | `6b018c4fc5b3653250c5e7e988953467fab0f7b3d0771e1a76a54d94a88c3772` |
| packet47-native-control-transport/standalone-r3.json | `738e0a1a307eb739d48a8e6f939ef7638c0e893bb149c9d72b3cb5281dee3a2b` |
| packet47-root-owned-coupling/r1.json | `42353526af1209921df04baf09306e5ae5216aa8fb0a66196658cc21408a3156` |
| packet47-root-owned-coupling/r1.log | `a16ea2565c0d665a0e725d1a2723a7ab00e65ad0c810f71284e51dd6abd63deb` |
| packet47-root-owned-coupling/r1-timestamp-correction.json | `1c7547d7996c43fd4d52ede4ed0008ad6f4ac89dfe147190642694ecbb3fe690` |

The coupling receipt hashes its actual debug runner, paired Rust CLI and native
executable; these hashes do not substitute for later rebuilt binaries or seals.

## Hosted and live boundaries

PR159 was merged after all 85 exact-head workflows passed. Its post-merge checks
were still pending at the last bounded inspection. PR160 was updated with main
at source-identical head 1eb6a81e8281dbb0256c683eaea579f590e07f7c; its new-head
hosted checks and the revised B6 Windows checks remain unverified here. The
03:49:20–03:52:20 UTC hosted inspection window is closed. This new stop source
has no claimed post-push hosted result in this record.

Native UI pilot18 remains 0/7 actions and 4/8 captures with Accessibility
untrusted. No permission query, UI launch or physical live job was retried.
Live app/guest stop, guest flush, supervisor crash/SIGKILL recovery, CLI start and
release gate counts remain unproven. All 29 criterion policies remain unchanged.
