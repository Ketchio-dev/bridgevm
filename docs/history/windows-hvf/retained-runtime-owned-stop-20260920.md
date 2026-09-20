# Exact-owned stop controls for retained HVF runtimes

## Unsafe boundary removed

An `HvfEngineSession` can outlive its library row while an app-owned launch is
still active. The retained control screen and the advanced runtime card both
previously reached an unqualified `stop()` method. That method could fall back
to a disk-path liveness lookup for an attached process, so those surfaces did
not express the same exact-ownership rule already used by the dashboard.

Source `213f3ac29b1d0748402fef49632373b72d13ee36` removes that unqualified API and
makes all three user-facing stop surfaces share one admission action:

- a stop is enabled only while the session retains an exact
  `HvfOwnedRuntimeIdentity` and no lifecycle transition is active;
- the action captures that identity and passes only its token to
  `stopOwned(expectedToken:)`;
- a pending start, missing owner, attached external runtime or ownership change
  is refused with a truthful result instead of discovering or terminating a
  process by disk path; and
- the dashboard, retained control and advanced card expose the same disabled
  guidance while no safely owned target exists.

The lower-level exact-token check remains authoritative at execution time, so a
UI state change between presentation and activation cannot expand ownership.

## Deterministic verification

Focused tests cover exact identity forwarding, refusal of an external
attachment without invoking the stop closure, pending-start refusal, an
ownership race, presentation for busy, stopped and unowned runtimes, and the
absence of guest-shutdown writes or new process probes for an arbitrary token.
Structural budgets pass with new files registered at their actual line counts
and no existing ceiling raised.

The complete source-tree project check ran all executable, app, security,
documentation and structural steps. Rust reported 1,018 own-HVF tests passing
with one existing intentional ignore and 381 probe tests passing. The four
Swift shim suites reported 425, 836 with the two required live-only skips, 62
and 109 tests. The only failed step was the expected capability-freshness gate,
because `tested_commit` still named the parent checkpoint before this record.
The retained 7,704-line log SHA-256 is
`5eae7985ecc181fd1af36a161a206cd51348c628de5003dea39cc474c30f958c`.

This checkpoint proves deterministic host-side stop admission and exact runtime
ownership. It does not prove a live Windows shutdown, guest flush, interruption
recovery, release readiness, performance or graphics behavior. A9, A11, A14,
A19 and B6 remain OPEN, product state remains Engineering Preview, and 3D
remains outside the release path. The running user VM was not stopped or
mutated.
