# Native runner cancellation and query evidence — 2026-09-16

Classification: historical deterministic evidence. Current capability state is
owned by the [registry](../../../capabilities/windows-hvf.json); A11 remains OPEN
and the product remains ENGINEERING_PREVIEW.

## Tested source and behavior

Source commit: `0a91061270031c5ca96fa2e2d70f266a49e64597`. The full check ran against 37 frozen staged
inputs while HEAD was `b90302c20e985ba95600b30ef825cbdabc57e311`; the resulting
source commit has exactly that tested tree. No new check execution at the
committed SHA is implied. Its three conclusions were committed separately:

- Preserve the earlier native observation history in a dated record.
- Preserve bounded B6 synthetic-query failures and actual child exit codes.
- Keep typed-runner disk and vars leases through owned helper and swtpm teardown.

Typed-runner TERM/INT now latch cancellation across reset generations. Every
direct child is retained from spawn until an actual wait result observes its
exit. The helper is stopped before swtpm; PreparedVm ownership outlives both.
Each child gets two seconds after TERM, then two seconds for KILL/reap. An
unconfirmed wait or deadline retains the owner and leases while observing; it
does not return successful cleanup. Startup key delivery and socket readiness
share the existing five-second monotonic budget. Legacy stdin key reading occurs
before any child exists. Keys are not written to arguments, logs or receipts.

B6 keeps its five original assertions and twenty-second query deadline. Cases
are labeled, post-exit output snapshots are bounded, and a query expected to
find no button skips only the additional provider-tree diagnostic. The two
production query scripts are unchanged. Post-spawn metadata failure still enters
owned-child cleanup; unconfirmed writers prevent raw-file deletion or capture.

## Observations and retained failures

| Check | Observation | Limit |
| --- | --- | --- |
| Initial full project check | FAIL, 194.16 seconds | Duplicate-branch lint and changed early-vTPM-exit diagnostic; frozen inputs unchanged |
| Corrected full project check | PASS, 196.67 seconds | Deterministic project checks; no release or guest result |
| Focused Rust runtime/runner suites | 87 passed; one ignored entry invoked by its parent subprocess test | Includes original suites and harmless actual-runner fixtures |
| Initial B6 headless run | FAIL | Nested PowerShell query lost expected exit code seven |
| Corrected B6 headless run | 66 checks passed in 1.967 seconds | Local official PowerShell 7.6.6 ARM64; Windows PowerShell 5.1 remains unverified |
| Independent cooperative TERM fixture | PASS, 0.064 seconds from request to runner exit | Fake helper/swtpm and tiny owned files |
| Independent ignored-TERM fixture | PASS, 2.075 seconds from request to runner exit | Same fixture boundary; no guest shutdown or flush proof |

The independent fixtures challenged disk and vars separately while running,
during helper teardown, and after helper exit while swtpm still held its release
channel. Every competing writer was refused. Both kernel exit observations
preceded final lease acquisition; owned temporary fixtures were then removed.
The copied runner binary SHA256 was `3f02f3c1fec0b7e30c109a0667b4e4874615ca2156aeec38a390c19a95182cae`.

A separate test deliberately reaped its own already-exited harmless child to
produce ECHILD. It verified the uncertain owner still held both media leases
and had not reached TPM teardown. This proves that wait-error retention branch;
it does not deliberately exercise four-second reap-deadline exhaustion or late
recovery. A review also found and corrected a test race by waiting for complete
marker contents before cancellation. Initial compile/setup and long socket-path
fixture failures are retained; the early incomplete-peer cleanup record is not
claimed as verified cleanup.

Hosted run 35048963776, preceding these changes, failed a managed B6 query timeout.
Its exact stalled stage and cause remain unknown. One unchanged failed-job retry
was requested; its result was not read in this checkpoint. The passing local
PowerShell run does not retroactively establish a cause or a Windows UI pass.

## Evidence identities

The private development evidence directory retains the original logs and receipts
under the following names. They contain no guest disk, vars image or vTPM key.

| Receipt/log | SHA256 |
| --- | --- |
| packet46-full-project-r1.log | `82482cacfb5cc05384a4903d1f643388300fd899912c226ff8651e73dc6c8f05` |
| packet46-full-project-r2.log | `511760f826bab62b40691c92db52c52822a7ad93dded7031d4c7e778df7d35d4` |
| packet46-focused-r2.log | `614b8238f3c9bce50a9a48b9d903170871c9d09b1b8ae67319ab5979592e7c32` |
| packet46-b6-headless-pwsh/headless-r2.log | `1ebc5cc4e4c1b37811f26a6ece69d1a01a7e7fdeeefd1d579207e6a7120bb41f` |
| packet46-root-lifecycle-review/held-r2/receipt.json | `495fb0297c8831b87f1bd19977d29c3a471662516e784471fe68c0efc5eed74e` |
| packet46-root-lifecycle-review/ignore-r2/receipt.json | `d041fa90fe165d7c66576b2430a760b30eae27df9dd1d0d80caca12037f18f0e` |

## Remaining work

CLI start/stop, the app-to-runner ownership/completion channel and app teardown
confirmation are not implemented by this checkpoint. Supervisor SIGKILL/crash
can bypass in-process cleanup; no guardian or completion proof is claimed.
Actual guest flush, Windows behavior, native UI actions and exact pushed-source
hosted checks remain separate requirements. Native UI pilot18 still has zero
of seven actions and four of eight captures because Accessibility was untrusted.
All criterion states, thresholds, known-defect wording and product wording remain
unchanged. Earlier observations are preserved in the
[native runtime history](native-runtime-observations-through-20260916.md).

## Windows fixture follow-up — 2026-09-16

Source `9b0b3d2e7c6ce082d78ed249d8482082ab1f4405` retains the preceding runtime behavior and corrects
the headless B6 fixture closure binding. Hosted runs 35053123614 and 35053123616
at earlier head `9d3241ab8c4c6f9ec8cbe71779a3f885920fa70e` both failed in
the first fake-child disposal operation because `Disposed` was absent. They had
not reached actual child processes or UI queries. The exact Windows PowerShell
5.1 variable-resolution mechanism is not established by those logs.

Fake operations now capture a directly scoped typed state parameter before the
factory closure returns. Seven new checks exercise two independent adapters
after caller variables change, including observation, clocks, kill, wait and
disposal. All preceding assertions and production query/process/evidence bytes
remain unchanged. Both local invocation styles passed 73 checks on PowerShell
7.6.6; this is not Windows PowerShell 5.1 validation. Revised hosted results are
still required. No timeout, threshold or failure was relaxed or rewritten.

The complete project check passed in 219.54 seconds on three frozen staged
inputs while HEAD was `dabcb995eb70d75d70b194766d60f27c97285a58`; source above
has the exact tested tree. The private receipt is `packet47a-full-project-r1.json`,
and its log SHA256 is `6988bdeb592d5a1b455ffbe3a6fbac9fc92774085894871e7dc26d3b8f2c545e`.
The focused source receipt is `packet47a-b6-fixture-isolation/source-ready-r1.json`.
This is deterministic evidence and does not close A11 or the remaining UI/guest
requirements.

PR159 was merged after all 85 exact-head workflows passed. PR160 likewise passed
at its preceding head, but branch protection required updating it with main;
the source-identical integrated head requires fresh hosted verification. No
protection was bypassed. CLI start/stop and the framed app/runtime completion
connection remain unfinished at this checkpoint.
