# B4 follow-up: current session is slow on current and retained code (2026-08-30)

## Status

Two fixed 20-lane follow-up campaigns failed the unchanged 250 ms p95 limit.
Both still landed every click and recorded zero rendering/package regressions.
An exact replay of the previously proven source was just as slow as the current
head, so the earlier hypothesis that a recent source change alone caused the
regression is retracted.

The retained B4 proof is not rewritten: job `20260829-215356-7576-17586` at
`080462846acbe4cb784bd9b532d7cd39921aa549` remains 20/20 with p95 245 ms. The
two failed follow-ups below also remain in the record and are not promoted as
new B4 proof.

## Fixed-campaign comparison

| campaign | sealed BridgeVM commit | landed | renderer/package regressions | pointer p95 | READY mean | forced cleanup | lanes with fence-poll stall |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| retained proof `20260829-215356-7576-17586` | `080462846acbe4cb784bd9b532d7cd39921aa549` | 20/20 | 0 | 245 ms | 18.31 s | 0/20 | 0/20 |
| current-head follow-up `20260830-061513-18964-30182` | `6048ebb084e7d1e2b1b78c7dcbe546b47c1c6e51` | 20/20 | 0 | 808 ms | 43.73 s | 20/20 | 19/20 |
| exact retained-code replay `20260830-063655-32633-32243` | `080462846acbe4cb784bd9b532d7cd39921aa549` | 20/20 | 0 | 839 ms | 43.45 s | 20/20 | 19/20 |

The two follow-ups used the same submitted input manifest as the retained pass:

| sealed input | SHA-256 |
| --- | --- |
| input manifest | `9d5844483ef72d9c240e5ccff473089b4b51b2533e232b95f322503fa4b19f94` |
| prepared pointer image | `7385d2005f3c48c40519a2d9cf2b9f975a3763ad8f37e84937d5ddf8429b0f8b` |
| prepared pointer vars | `bec224d27c8681d2db69583e933e2d99b6fa5265d91d37373cb7a2c8b71853cd` |
| driver package tree | `c46486e5643317002b8848ac855324bc397e6ee6ac6e2987da7af0c61c6a3860` |

All three receipts report `Mac17,9` and macOS 26.5. Every follow-up lane
recorded host fire, guest press and release, one target click, `stuck=0`, and a
visible active-IOSurface change. A scan of every follow-up `run.log` and
`virtio-gpu.jsonl` found no `Illegal resource`, `context error reported`,
`Illegal command buffer`, all-black failure, or invalid-baseline failure.

The exact replay latencies were:

```text
839 572 700 435 737 452 352 737 270 479
578 258 594 510 888 414 436 168 445 794
```

## Broader slowdown evidence

This was not only a pointer-response shift. The retained campaign reached
`BVAGENT READY` in 15.92-19.03 seconds (mean 18.31 seconds). The current-head
follow-up took 41.70-45.79 seconds (mean 43.73 seconds), while the exact old-code
replay took 41.34-45.54 seconds (mean 43.45 seconds).

The retained lanes shut down within the normal cleanup boundary. Every lane in
both follow-ups remained alive after the guest shutdown request and was
terminated by the existing 60-second cleanup fallback. Nineteen lanes in each
follow-up also emitted a fence-poll stall; the retained campaign emitted none.
These post-sample shutdown outcomes do not change the recorded pointer
classification, but they are retained because they show that the session-wide
runtime was behaving differently.

A read-only host snapshot at `2026-08-30T11:54:18Z` found AC power, no recorded
thermal or performance warning, 83% system-wide memory free, 5.31 MB swap in
use, GPU `recoveryCount=0`, and about 12% GPU device/renderer utilization. A
five-second disk sample observed 0-0.72 MB/s after its cumulative first line.
Those observations rule out the specific warnings they measured; they do not
identify the cause of the temporal host/session change.

## Conclusion and next boundary

The exact-source replay falsifies a recent-code-only explanation. It does not
prove which unsealed host, Hypervisor.framework, WindowServer/CGL, scheduler,
or other session state caused the difference. An eager guest-RAM hypothesis
was therefore abandoned and its experimental edit was reverted rather than
committed.

The current head is not claimed to improve B4 latency, and neither failed
follow-up is substituted for the retained passing receipt. The secondary-vCPU
drain gate is restored to its previously proven default, but that conservative
restoration is not presented as a cure for the observed session state.

The next comparable release-cut measurement should be one predeclared fixed
20-lane run after a normal host-session reset, with the same immutable inputs
and unchanged 250 ms threshold. A restart is an environmental reset, not a
reason to discard these failures or retry repeatedly until a pass appears.
