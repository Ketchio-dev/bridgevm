# Native owned startup — 2026-09-16

Classification: historical deterministic evidence. Product state remains in the
[registry](../../../capabilities/windows-hvf.json). A11 stays OPEN and
ENGINEERING_PREVIEW is unchanged. This follows the [callback scope record](native-callback-scope-20260916.md).

## Implemented boundary

The native and paired Rust CLI now route `start ID` through the already-running
app owner and retained LibraryModel. The exact installed HVF registration,
canonical library, app instance, operation ID and normalized saved-config digest
are bound before admission. No app/window starts, attachment, install, legacy
wrapper fallback or arbitrary runtime path is admitted by this command.

The ledger retains at most 256 owner-lifetime records without eviction. Exact
retries return the original ticket; changed binding conflicts. Reservation is
committed to both session and ledger before publication or worker effects.
Removed-but-retained sessions, other runtime owners, installers and storage work
remain conflicts. Only the exact session/reservation is excluded from its own
work guard. Saved/model/library/owner identity is rechecked at effect boundaries.

A detached retained worker owns readiness, existing-key lookup, preparation and
spawn. Thirty seconds from admission is fixed; a CLI retry never renews it.
A pending synchronous call is not falsely cancelled or released at the deadline.
Changed configuration invalidates the effect fence. A late already-spawned
process is adopted and sent through confirmed owned cleanup. Startup success
requires validated READY and generation-zero helper-start evidence before
terminal/protocol failure. Historical startup proof survives a later stop.
The proof's manifest hash is distinct from the normalized saved-config hash.

Existing-key lookup preserves account/service/query scopes and never writes,
creates a key or probes another backend after failure. A resolved backend is
reused; otherwise read-only entitlement inspection selects one candidate.
BridgeVM key operations share a nonblocking guard. Legacy interaction policy is
saved/disabled/restored before key consumption; restore failure discards the
result and disables the access service. Data-protection lookup uses a fresh
no-interaction context. Historical backend fallback was not persisted; global
legacy policy also affects callers outside this guard. Injected tests do not
prove platform-wide prompt suppression. No host credentials were read in tests.

TPM confirmation/modal and snapshot work reserve the session before effects.
Restore/reset transfer the reservation into local ownership through the entire
lifecycle call, so closing the view cannot release in-flight work. Snapshot
workers revalidate library admission before directory/process effects. Preparing
and pending cleanup are visible in the app; Start and pre-spawn Stop no longer
present actions that preparation cannot accept. See the [start contract](../../app-cli-start.md).

## Retained validation, including failures

| Check | Result and provenance |
| --- | --- |
| Existing-key isolated shim | 18/18 PASS; first run 17/18 failed because the fixture inspected a context after invalidation; query-boundary observation corrected |
| Wire same-PID regression | RED then GREEN; response now rejects helper PID equal to retained runner PID |
| Private socket suite | 22/22 PASS, 16.787 seconds; all 49 recorded source/test inputs unchanged |
| Product build before final refinements | PASS, 3.379 seconds, source hashes stable |
| Combined control shim R1 | FAIL at test compilation, 15.356 seconds; new assertion unavailable in existing shim, identical predicate rewritten using supported assertion |
| Combined control shim R2 | Overall FAIL, 718 PASS / 1 FAIL / 2 required live-only skips, 70.455 seconds; all recorded inputs unchanged |
| R2 single failure | Old parser test still classified newly supported start as unknown; changed only that verb to an actually unsupported one |
| Focused corrected parser and start CLI | 9/9 PASS, 1.173 seconds; same compiled R2 product module, corrected test inputs stable |
| Independent model, key, wire and runtime reviews | No remaining bounded static blocker; exact reviewed hashes retained privately |
| Frozen full project R1 | FAIL in 184.132 seconds: two steps retained obsolete start-as-unknown/help expectations; all 3,187 inputs stable and all three shim suites passed (425, 719 plus two skips, 62); failed run retained |

Both R2 actual CLI cases passed: paired Rust start/native stop and native
start/paired Rust stop, each with v1 status, duplicate-start refusal, one launch,
exact saved-config/run/manifest binding and historical ticket query after stop.
They use the production app owner/router, retained session worker and Rust
runner with harmless synthetic helper/TPM children and synthetic key bytes.
Independent kernel exit witnesses confirm both child exits. No App.main,
WindowServer or Windows guest is exercised by these deterministic fixtures.

The new source and final full-project seal are recorded in the registry/private
checkpoint. Failed combined/project/package runs remain failures; later checks do not rewrite them.
Frozen full project R2 passed in 220.312 seconds with all 3,187 inputs, HEAD and index unchanged. Log SHA256: `184967f8491ba73556af38ca5a1f00d88b883a65d2e9791d4410a4c2f8f97e14`.

## Predecessor hosted evidence and remaining scope

Exact PR161 head e5024e1b05d8e7d4629720bbd50b42d754619b74 now has successful
hosted Windows receipts: native tip 35063418391 (155 headless, seven DPI and five
original UI cases), caption 35063418442 (155 headless, four caption and five tip
cases), and inventory 35063418385 (48 lifecycle plus three original cases on
Windows PowerShell 5.1 and PowerShell 7). The complete head snapshot was 74
successful and eleven queued/running workflows when this bounded window closed.
PR161 remains unmerged until all required checks are verified. Its prior six
failures and scope regression remain in the callback record. PR159/160 merge and
post-merge verification/branch cleanup are already recorded there.

This is deterministic process/transport and hosted diagnostic evidence. Live
app/guest startup and stop, guest application flush, supervisor crash recovery,
legacy Keychain platform behavior and native UI pilot18 remain unproven.
Pilot18 still has 0/7 actions and 4/8 captures with Accessibility untrusted.
No release criterion, sample count, threshold, product claim or guest deviation
is promoted by this packet. Current exact-source hosted validation is pending.
