# Legacy graphics opt-in hardening

## Live observation

A user-visible Windows sign-in frame showed the wallpaper, pointer, avatar shape
and password field while expected account text and surrounding sign-in glyphs
were absent. The supplied 2644x1696 PNG has SHA-256
`0b576d547a0a636abae5d2b9b7834ba552cc7a8bdebd251a1618cd89fd0ab1dd`.
The private image is not committed.

The active VM command line included `--virtio-gpu-3d`, device ID `1050` and the
VirGL trace protocol. Its legacy `vm.json` omitted `experimental3DAllowed`. The
session was launched by an older development preview app, not the newer signed
app under `/Applications`. This is a single live observation. It does not prove
the shader or composition mechanism and does not close B6.

## Product correction

Current library launch construction already interprets a missing graphics policy
as 3D-off. Source `bfbbeb3c9aed703e7124c5a8cafb66e00a735897`
closes the remaining default-on surfaces:

- a raw `HvfEngineConfig` no longer allows experimental 3D unless its caller
  explicitly grants that capability;
- the runtime view initializes its 3D launch choice to off; and
- the standalone Graphics Lab retains the experimental toggle but begins on the
  basic display path, requiring an explicit opt-in for each new session.

Explicit saved `experimental3DAllowed=true` data is preserved for Graphics Lab.
Existing product VMs with a missing or false value emit no 3D launch arguments.
The running user VM and its registration were not stopped or mutated.

## Deterministic verification

Three focused Apple XCTest cases passed: missing legacy policy stays 3D-off,
Graphics Lab begins 3D-off while retaining the opt-in surface, and an explicit
experimental opt-in still emits the 3D launch argument. Structural budgets pass
without raising an existing ceiling; the new test file is registered at its
actual 27 lines.

This hardening prevents a future or diagnostic constructor from silently making
3D the default. It does not repair the observed glyph defect, prove the 3D-off
login frame, replace the B6 matrix, or promote any capability. B6 remains OPEN,
3D remains a future Graphics Lab path, and product state remains Engineering
Preview.
