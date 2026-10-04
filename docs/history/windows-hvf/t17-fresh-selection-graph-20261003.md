# T17 failed selection and attempt-local graph — 2026-10-03

This records one failed physical pilot and a source repair under development.
No criterion, threshold, product state or capability is promoted. The
[preceding failed pilot](t17-native-chooser-timed-pilot-failure-20261003.md)
and [required hosted failure](t22-loader-admission-hosted-full-failure-20261003.md)
remain failed experiments.

## Preserved physical failure

Source `91e2f198e6a5d0b083024f4a288c9bb33b94e07e` had a full local project
check with 44 outer PASS, zero FAIL, and required exact-source hosted CI GREEN.
The [hosted full check](https://github.com/Ketchio-dev/bridgevm/actions/runs/37152052514/job/111287774589)
recorded 43 applicable outer PASS and the explicit unavailable Linux-target
skip; standard hosted Linux compilation passed separately.

Pilot `codex-t17-91e2f198-shared-chooser-pilot-r1` was submitted once at
23:00:54 UTC and failed with exit 1. Its single run had passes 0, failures 1,
and cleanup verified true. Receipt SHA-256:
`36019e19d43e041d28d5239513575b6e326551226e1ecf98d214acb588cc56fc`.
Failed body SHA-256:
`e5900e56889f38808c503a9cb2e7a4990ceace6f9437c51481ec493377fc76c8`.
Original receipt CI fields stay unchanged; separate hosted source proof does
not make this failed pilot release evidence.

The failure was `input-selection-failed` at `accept-selection`. A selection
button lookup nested through snapshot-attempt into a relationship read that
returned after the shared deadline. Selection-ready took 13,399.596 ms with
4,935.268 ms remaining; acceptance took 4,936.335 ms and returned with
-1.081 ms remaining. These are whole-operation durations, not isolated
single-AX-call measurements. No successful selection press, first READY or
guest-runtime pass was established. Earlier frontend stage labels do not
prove guest installation or boot.

A retained early request matched the final host stamp and failed result body.
This authenticates that record association, not failed-lane media. The genuine
sampler retained 26 samples, including 15 recording the app running and
frontmost, with zero sampled refusals. Continuous observation was not proven.

Root missed both planned timed captures. The driver was started at submission
age 320.687703 seconds, beyond its fixed 270-second startup age limit; the
actual tool returned exit 1 and generic ValueError. No driver-run or batch
outputs were created. The exact original exception cause is not isolated by
the generic output. There was no clock-origin change, forced late capture or
automatic retry. An untimed running-lane read after terminal state was refused.

## Source change and limits

Static reading shows that the preceding selection lookup walks the complete
application graph and then rereads relationships in the selected panel subtree.
Readiness and acceptance each perform that lookup. This duplication is proven
in source; it is not an isolated live cause for the measured delay.

The successor captures each unique node's relationships once per snapshot
attempt, then reuses them to resolve the panel and its button. Both stages
still start fresh lookups; transient retries discard the previous graph.
The shared deadline and native before/returned checks remain. No synchronous
AX call is advertised as preemptible.

The complete eligible-panel search still rejects distinct duplicate owners.
An Open button must have exact identifier `OKButton`, role `AXButton`, and
be distinct-match unambiguous. Nodes also reachable from the application
without passing through the selected panel are excluded. This prevents
cycles or shared foreign-window references from yielding a selectable button.
It conservatively tightens the previous identifier-only first-match behavior;
legitimate native aliases might also be refused. Graph domination is a source
ownership policy, not a proven native ownership contract. There is no global
button fallback or readiness-button cache.

Focused native34 and full helper371 XCTest/41 Swift Testing cases passed.
Exact f5 full project check failed: 43 PASS/one capability freshness FAIL, exit1;
all4148 source records unchanged. Required hosted job111319890446 failed the same
freshness check. A metadata-only successor binds tested f5 code; new checks remain pending.
