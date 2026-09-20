# Unavailable HVF detail fails closed

## Removed fallback

The library can temporarily retain an older `ControlModel` while that model is
busy, running or owns a lifecycle operation. If the registration changes engine
in that interval, the selected detail can still describe own-HVF while the latest
saved configuration can no longer create an `HvfEngineSession`.

Both the dashboard advanced route and the ordinary detail route previously fell
through to `VMDetailPanel` when that session lookup returned nil. That generic
panel owns historical backend Start and Stop actions, so a transient metadata
mismatch could bypass the exact native-session ownership boundary.

Source `b9c4355f0b6e9e9812e22ebde86441593f855624` removes that fallback:

- dashboard advanced detail uses one engine-aware content router;
- own-HVF with an exact session presents `HvfEngineView`;
- own-HVF without one presents a read-only unavailable state with library reload
  as its only action; and
- Fast VZ and QEMU continue to use `VMDetailPanel`.

The unavailable state exposes no start, stop, guest-command or deletion action.
It also does not manufacture a replacement runtime identity.

## Deterministic verification

The first focused run exposed a test-inspection error: the new router body had
not been explicitly evaluated, so the unavailable view was not observed. After
that correction, the two new cases passed. A broader existing session-store run
then showed that wrapping the ordinary detail route changed its intentionally
shallow SwiftUI value contract. The implementation was narrowed so the ordinary
route retains its established structure and inserts only the own-HVF nil branch.

The final focused run passed all 12 cases: two new routing regressions and ten
existing runtime-session lifetime, replacement and retention cases. The stale
own-HVF case creates no runtime session and exposes neither `VMDetailPanel` nor
`HvfEngineView`; the ordinary non-HVF case still exposes the generic panel.
Structural budgets pass without raising a ceiling. Both new files are registered
at their actual sizes, 36 and 46 lines.

This proves deterministic fail-closed view routing only. It does not prove a live
metadata race, Windows shutdown, interruption recovery or release readiness.
A9, A11, A19 and B6 remain OPEN, product state remains Engineering Preview, and
the running user VM was not stopped or mutated.
