# Truthful graphics-mode presentation checkpoint

## Observation

A live Windows login screen rendered its wallpaper, pointer, avatar and input
box while the user name and instructional glyphs were blank. The host-owned
`Guest windows` overlay remained legible. The running command line and launch
preflight both proved that this ten-hour development session used the historical
`--virtio-gpu-3d` VirGL path. This matches the retained B6 known defect; it is
not evidence that the release display path has the same failure.

The saved VM predates the 3D-off policy field and its user-owned display name
still contains `HVF · 3D`. Current source correctly treats the missing policy as
false for the next launch, but that saved policy cannot describe an already
running process started by an older app. Displaying only the saved value would
therefore make a false statement about the live session.

## Deterministic correction

Source `1bedc9e2b48751cef7ee67d43deddacd82b8f666` adds a typed dashboard
presentation boundary with three distinct evidence classes:

- a stopped VM shows the policy for its next launch;
- a process launched and retained by the current backend shows the frozen basic
  or Graphics Lab policy used for that launch;
- a pre-existing process that the backend did not launch shows `기존 세션 · 모드
  확인 불가` regardless of the saved setting.

The dashboard keeps the user's display name unchanged and presents the graphics
status beside the engine label. It does not inspect names for capability tags,
infer a live mode from persisted configuration, or classify an attachment as
3D-off merely because new builds default to that policy.

## Verification and limits

Four focused classification tests cover the missing-policy default, both owned
live modes, the stopped experimental policy and all saved-policy values for an
unverified running session. Structural budgets pass with the two new files
registered at their actual 41-line and 34-line sizes. The complete local project
check passed, including capability and documentation checks, default and Venus
Rust suites, 381 probe tests, native CLI and UI-driver contracts, and all four
Swift shim suites: 425, 827 with the two existing live-only skips, 62 and 109.

This is deterministic presentation evidence only. The observed live VM continues
under its original process and was not stopped or mutated. The correction does
not prove a rendered guest frame, repair the experimental glyph defect, close
B6, A9 or A11, or change the Engineering Preview product state. 3D remains a
Graphics Lab future path outside General Preview and v1.
