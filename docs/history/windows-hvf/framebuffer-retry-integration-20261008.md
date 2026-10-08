# Current-source framebuffer retry integration — 2026-10-08

Evidence: deterministic real-view tests and static source review. No live
Windows/display smoothness or criterion claim.

PR304 retained a correct retry repair at
`6e86454c761c80a37a3a7114a42428351301c411`, but remained draft while main evolved.
This successor applies its exact four-file production/test patch to main
`6a02a8b87a8d4f64b32042720d7d063fd4093c2a`, without resetting current session,
input-focus, pathname identity or IOSurface behavior. Historical PR and its
[failed experiments](framebuffer-image-retry-20261004.md) stay unchanged.

The view commits a copied frame sequence only after valid image construction
and assignment to a present layer. Provider/image construction failure retries
the same sequence on the next tick; success still deduplicates unchanged frames.
Copied-buffer ownership and both seqlock reads are preserved. Layer assignment
is submission, not proof of compositor presentation or physical scanout.

## Fresh current-tree evidence

- Native retry2/0, then focused framebuffer/file-identity/focus/accessibility/
  IOSurface set13/0; exact pixels and frame-ready accessibility asserted.
- Moving sequence consumption back before image construction causes both retry
  cases to fail at the second-tick image assertion. Negative-control mutation
  was removed before the restored13/0 run.
- Existing display-click source contract4/0. Optional factory initializer
  extraction preserves the original pointer/accessibility assertions.
- View budget447→424; factory28/test78 registered at actual size. No existing
  ceiling increased. Full local and exact hosted checks remain required.

Native test startup printed duplicate UniversalHID class warnings from installed
system/developer frameworks. Tests passed; no framework deletion or host change
was attempted. Prior6e checks are history, not validation of this successor.
A9/A11/A19 and product state remain unchanged.
