# A9 first-ready terminal observation — 2026-09-20

This is an engineering checkpoint, not A9 completion evidence. A9 remains
OPEN, the product state remains Engineering Preview, and 3D remains outside
the release path.

## Measured gap

Physical pilot `t17-6af35530-dashboard-transition-memory-r16` proved product
creation, installation and Secure Boot provisioning, then failed because the
installed guest produced no first `BVAGENT READY` signal. The product helper
waited the complete 600-second bound while looking only at `run.log`. If the
owned runtime had already failed or terminated, that wait did not preserve the
product UI state that distinguished the failure.

## Deterministic change

The runtime status value now has the exact Accessibility identifier
`bridgevm.windows.runtime.state` and a stable, nonlocalized value:
`start-pending`, `stopped`, `booting`, `connected`, `stopping` or `timed-out`.
The install and import product helpers share one first-ready monitor. It:

- succeeds only for the existing `BVAGENT READY` evidence, plus the already
  accepted proactive `PONG` in the install flow;
- observes start admission before treating a later stopped state as terminal;
- fails early when the owned product app exits, startup reports an explicit
  failure, or an active runtime stops, starts stopping or times out;
- retains the existing bounded `run.log` hash and marker counts; and
- hashes bounded startup-failure text instead of copying private UI text into
  a receipt.

The optional Accessibility read reacquires the complete bounded application
graph. A traversal or attribute error remains an error and is not converted to
an absent optional element.

Six focused first-ready state/timeout/privacy contracts and two runtime-state
mapping contracts pass locally. The full ProductE2E suite and structural
budgets also pass. Exact-head full project and hosted checks are still required.

## Limit

This change improves failure attribution and avoids known terminal waits. It
does not explain why the previous Windows guest lacked `READY`, prove that a
new guest boots, or satisfy any A9 sample. A new signed physical-Mac pilot is
still required after exact-head hosted verification and when the machine has no
user-owned VM session that the pilot could disrupt.
