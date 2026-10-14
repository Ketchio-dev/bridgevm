# Retained controls and input cleanup — 2026-10-14

Evidence rank: **test**. This third isolated integration follows
`828b7029bd37f4a717cfc03a0f237e72f23c43b0`. Focused counts overlap suites
and are not a unique aggregate. Complete combined checks remain required.
No WindowServer interaction, guest behavior or release criterion is proved.

## Retained control navigation

The current sidebar no longer included the existing retained-control entry.
When an active runtime or installation lost its saved registration, selecting
another VM left no route back to its captured controls. Two baseline methods
failed four assertions. The sidebar now includes the existing entry and
routes selection through the exact captured identity, including stale-token
refusal, replacement isolation and terminal-only dismissal.

Native15 and actual-shim15 cases passed. The tests construct the actual
SwiftUI value graph and invoke its selection binding; they do not render or
click a native window. Source conclusion: `aa0fb80f1db19a0b2bf4ba2ab44b90d10214eb2a`.
Patch SHA-256:
`cbdfe73ff18f45ed14b42c7681819a8d0b2254b858fcd2e09cfc993b157be96c`.
Generic FastVZ/QEMU process ownership remains a separately investigated scope.

## Focus loss and warnings

The own-window resignation callback ran on the main queue but deferred input
cancellation into another task. Immediate marked-text completion could
therefore append TEXTINPUT to the old target. The two-method baseline failed
three assertions in one method; its unrelated-window control passed. The
callback now cancels synchronously on its guaranteed main queue while keeping
window, generation and weak-owner checks. Native52 and actual-shim52 cases
passed. Initial fixtures used Apple's internal temporary-directory property
despite TMPDIR; those runs are retained, the fixtures now use the existing
environment-aware helper, and baseline/final proof was repeated on SSD.
Source conclusion: `79a40594`.

The current overview also showed a warning count without a route to the full
existing details sheet. Two baseline methods failed four assertions with five
malformed entries, alone and beside a healthy VM. The compact warning entry
now opens the same complete details content. Three new and eight related
cases passed both natively and through the actual shim. This is constructed
host-navigation proof, not a rendered button click. Source: `7039d2c3`.

## One-shot pointer cleanup

A confirmed pointer press followed by a rejected text request failed ordinary
input and also blocked later mouse-up/target-cancellation cleanup. Two
baseline methods produced one failure with three assertions; healthy target
cancellation passed. The existing pointer cleanup is now shared with failed
or expired user requests. A failed router may drain only its one correlated
release request; ordinary and legacy input stay refused even after success.

Before appending cleanup, existing binding, service, READY and guest-reset
checks still cancel stale work. Rejected writes, invalid insertion receipts
and expiry block repeat cleanup. No request replay, new deadline, protocol
or success threshold is introduced. The actual relay test keeps an overdue
press in its wire slot over six intervals and permits cleanup only after its
terminal reply, proving it cannot overtake or retransmit that press.

Final native49, actual-shim43, actual relay1 and optimized transport2 cases
passed. Six existing reset cases were omitted only from the focused shim
selection because its large auxiliary class was not copied; they passed
natively, and all three new lifecycle/reset boundaries ran in the shim.
Initial fixture expectations, a relay import error and standalone-driver/shim
invocation errors remain retained; corrected runs do not erase them.

Source conclusion: `8fbc13186c5c777f3fe645cbbaa5eb43e1ff91f5`.
Patch SHA-256:
`18d89dbdde7a80a19fec3cb1e907e038f92e5693e81efc269d1ff83f9051da1d`.
Independent review and integration checks matched all12 source files,
16 evidence hashes and two fixture changes already present in the focus fix.
Structural budgets passed without increasing an existing ceiling.

This remains a best-effort release attempt. Host cancellation cannot retract
already appended commands; queued control work can survive an agent restart,
and cleanup can execute after the host deadline. No guest button-release,
application recovery or destination clipboard-consumption result is claimed.
A9, A11 and A19 remain OPEN; product wording and live gates are unchanged.
