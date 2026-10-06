# Secondary vCPU shutdown notification — 2026-10-05

Integrated source checkpoint `fd8e52a2cc7a34e22c6961f7345eefe59d7bdbc9`
repairs a lost shutdown notification in the own-VMM boot-probe runtime.
An Off secondary could read shutdown as false, then miss a notification
before entering its condition-variable wait. A never-started owner has no
published HVF handle to receive the later exit request, so joining could stall.

## Notification boundary and lock topology

[Rust's Condvar contract](https://doc.rust-lang.org/std/sync/struct.Condvar.html)
defines atomic mutex release when waiting and says notifications are not
buffered. `VcpuControl::notify_shutdown` now acquires the same state mutex
used by the wait predicate and keeps its guard through `notify_all`. The
shutdown atomic is still stored first by `shutdown_and_join`; serialization
closes the interval between the predicate read and entry into the wait.

The sole production caller holds no state, handle or platform lock while
notifying. The new guard drops before later handle-exit requests and joins.
The secondary owner drops its state guard before provider creation, execution
and final capture. CPU_ON already changes state under the mutex and releases
it before notification. This review found no new normal-path lock cycle.

The extracted eleven-line `vcpu_coordination/shutdown.rs` uses a poisoned
guard's `into_inner` only as a notification barrier. It neither reads nor
changes the PSCI state and does not clear poison. Owner/join failure policy,
the secondary loop, PSCI transitions and provider exit requests are unchanged.
Reinserting the old method and removing module registrations recovers the
original coordination source byte for byte; old test assertions are intact.
Its ceiling falls from 278 to 275 lines, retaining unsafe 1. The new helper
and 127-line fixture have unsafe 0; no existing ceiling increases.

## Paired fixtures and retained failures

The first four-case fixture revision gave 36 PASS / 1 FAIL on original
production and 37 PASS after an initial mutex barrier using `expect`.
Review found that `expect` would introduce an earlier panic on poisoned state.
A new poison control actually failed against that first repair, 0 PASS /
1 FAIL. That source and all earlier results remain recorded.

The final five cases, unchanged across paired runs, give 37 PASS / 1 FAIL
on original production and 38 PASS / 0 FAIL in both repaired debug and
release profiles: 33 existing tests plus five new cases. Fixture SHA-256 is
`a87eeed6e6168abd532a10977cb0bc4494106ad68afe1a76e07e91824b0b9afc`.

The lost-wake fixture holds the real state mutex across a prior false
shutdown read, then invokes the actual production notifier from another
thread. It observes early return while the guard is held and then performs
a real timed wait. Both original-production baselines observed the lost
notification. Other cases exercise an already parked synthetic Off owner
through actual `shutdown_and_join`, shutdown before waiting, unchanged PSCI
state values and the poisoned notification barrier. Threads are joined after
releasing guards and before verdict assertions; 500 ms and two-second bounds
are fixture scheduling/cleanup limits, not product acceptance thresholds.

These cases model the wait boundary using production synchronization objects
and methods. They do not execute the entire `secondary_vcpu_thread`, create
HVF vCPUs or prove every possible interleaving. The poison control does not
establish recovery of a whole secondary set with a poisoned owner.

Retained raw SHA-256 values are:

- R1 original: `a5d4d7282dc3e1738bcee1321e89ddfe4cf2ecb3b554e5dc8d7d706f0d124397`.
- R1 first repair: `01f1ce65256a219cda80ab18cad4bf4dbe75f600adac51dc7dc24c41ca3d3533`.
- R1 poison failure: `fb8272399d64d0728f0c58358416aa723fc1fb8949fc5b5ccb3c7be936f57dcb`.
- R2 original: `b288db88ecedf867b4ea21695fff99906f9eaf5719ced450138dd319ec3648e1`.
- R2 debug: `42b8937ada9206aa40c8c7b803c564f7a418d7901f1349be7c76e261890de3f7`.
- R2 release: `919b650fef6a0f8a4c1afbbca45362eb9a5eb2ad1765196a8789d269d3839bca`.

R1's test-registration file was reconstructed afterward and verified against
both historical receipt hashes; it was not separately retained at that time.
Existing owner-final-state tests pass 3/3 and PSCI tests 6/6. Focused Clippy,
formatting, budgets, whitespace and independent source/evidence review pass.
Integration preserved all reviewed nonbudget source bytes and resolved only
the appended budget registrations as an exact union.

## Limits and incomplete integration evidence

No real secondary-vCPU shutdown, VM or Windows workload was measured. The
W16 local project check remains FAILED at 37 PASS / 8 FAIL. Two hosted jobs
failed during runner acquisition. By 2026-10-05 22:18 UTC, hosted full had
44 PASS and manual CI 14 PASS; other failures keep validation INCOMPLETE. Successor
checks remain pending. Canonical capability fields, criterion thresholds,
known defects and product wording are unchanged; no live criterion or release
promotion follows.
