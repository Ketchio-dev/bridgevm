# Hosted pool starvation in a resource display test — 2026-10-21

This records a hosted failure of the eleventh candidate and a test-only repair. It
records automated tests. Product state, capability wording, criterion states, thresholds
and known defects remain unchanged.

## Hosted failure

`ControlModelResourceDisplayTests.testSuccessfulApplyInvalidatesActualOldReadAdmittedDuringApply`
failed in every hosted shim run of the candidate's two published heads, with and without
Thread Sanitizer. The fixture reported "Actual writer did not acknowledge within 3
seconds" and then threw. Every other BridgeVMControl test passed.

| Head | Run and job | Runner image | BridgeVMControl shim |
| --- | --- | --- | --- |
| ce7e5512 | CI 39557446601, job 119080425215 | macos-15-arm64 20260907.0337.1 | 1,442 passed, 1 failed |
| ce7e5512 | CI 39557446601, job 119080425321 | macos-26-arm64 20261013.0424.1 | 1,442 passed, 1 failed |
| ce7e5512 | Hosted full 39557445608, job 119080425087 | macos-15-arm64 20260907.0337.1 | 1,442 passed, 1 failed |
| a2f201c6 | CI 39560471509, job 119090236592 | macos-26-arm64 20261013.0424.1 | 1,443 passed, 1 failed |
| a2f201c6 | CI 39560471509, job 119090236661 | macos-15-arm64 20260907.0337.1 | 1,443 passed, 1 failed |

The same ce7e5512 source passed all 45 local stages on a 16-CPU host. The a2f201c6 hosted
full run 39560470764 was cancelled while queued, and its exact local full was stopped
during the contract stages. Neither is a result.

## Cause

The Swift cooperative pool has one thread per CPU, and GitHub's standard macOS arm64
runners have three CPUs. In this test, the product's gated writer and reader each block a
pool thread by design. The fixture's waits for the write acknowledgement and the read
capture each blocked another, so four blocked threads exceeded three. The read-capture
wait could not start until the acknowledgement wait timed out.

A temporary local harness kept all but N pool threads blocked and ran the same test body.

| Fixture waiter | 16 threads free | 3 free | 2 free |
| --- | --- | --- | --- |
| Blocks a pool thread (published heads) | passes | fails with the hosted message | not measured |
| Dedicated thread (this repair) | passes | passes | fails |

With two threads free, the gated writer and reader hold both. Resuming the awaiting
main-actor test also needed a free pool thread in local measurements. That held for checked
and unsafe continuations, `Task.yield` and `Task.sleep`. The test therefore needs three
threads unless product code changes. A GCD worker did not start while the pool was fully
blocked, so the waiter is a plain thread. These are local automated measurements. The
three-CPU runner size is GitHub's documented specification, not measured here.

## Repair

`waitOffCooperativePool` waits on a dedicated thread. The stale-read probe and the display
backend's write acknowledgement use it. Assertions, the interleaving and the 3-second
bounds are unchanged. Each bound now starts when its wait begins.

Six other test waiters, plus the registration backend from aa943048, still block a pool
thread while waiting. They have passed every hosted run and are unchanged. The harness
was never committed. The new head requires its own complete local and hosted checks.
