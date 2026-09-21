# A9 first-ready coherent AX snapshot — 2026-09-20

This checkpoint does not close A9. Product state remains Engineering Preview,
A9 and A11 remain OPEN, and 3D remains outside General Preview and v1.

## Measured failure

Exact-main physical job `t17-26c56ce2-first-ready-terminal-r17` passed all ten
artifact inputs, VM creation, source preparation, Windows installation and
Microsoft-only Secure Boot provisioning. Its first-ready observer then failed
closed while looking for `bridgevm.windows.runtime.start.failure`: the
AXIdentifier read returned generic AX failure `-25200` after the existing
three-attempt snapshot boundary. Cleanup was verified. The strict public
receipt passed repository verification at SHA-256
`b3b1c6715db81cc6f3abaf6fe8dfc1ddc052150e8e6a6ed50e733afd94fa4658`.

The observer previously acquired one complete application graph for
`bridgevm.windows.runtime.state`, discarded it, then acquired another complete
graph for the optional startup-failure value. A failure in the second traversal
discarded the first readable state and ended the 600-second first-ready wait.

## Deterministic change

Source `f7f7bf9d49b34e64164306d438220a24fc9cc2ea` projects both exact identifiers
from one bounded application-rooted graph. A clean graph may report either
optional value absent. If an identifier read fails while an expected value is
absent, absence is ambiguous and the original error is retained.

Only existing retryable generic AX failure `-25200` and invalid-element
`-25202` reacquire the whole graph. The first-ready observation uses five fixed
attempts paced by 500 ms, for at most two seconds between the first and final
attempt. Every other AX error fails immediately, and exhaustion returns the
final exact error with both sorted identifiers attributed.

Eight focused contracts cover complete projection, clean absence, ambiguous
absence, unrelated read failures, fifth-attempt recovery, fixed exhaustion,
non-retryable failure and stable attribution. The complete ProductE2E target
passed 123 XCTest cases and 33 Swift Testing cases. Structural budgets pass
without raising an existing ceiling.

The complete source-head project check passed every other executable, app,
security, documentation and structural step and correctly failed only stale
capability identity before this record. Its 7,716-line retained log SHA-256 is
`0adb4e084b874c509f62cc4d2889b913523997c2c99e463d38523342e34b9606`.

## Limits

This change needs another signed physical T17 pilot. It does not prove that the
guest emits READY/PONG, that Windows reaches a usable desktop, or that the new
AX bound survives the measured live transition. No criterion, threshold,
product state, graphics claim or sample count changes.
