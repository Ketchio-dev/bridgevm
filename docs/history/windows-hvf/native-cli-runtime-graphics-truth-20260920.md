# Native CLI runtime graphics truth

## Problem

The native app status protocol reported retained process ownership and connection
state but omitted the graphics mode accepted for that launch. A user could see a
defective frame from an older experimental VirGL session while the current saved
configuration correctly defaulted to 3D-off, without the CLI exposing that the
running process and next-launch policy differed.

The live legacy process observed during this work carried `--virtio-gpu-3d` and
`--gpu-trace-protocol virgl`. Its saved registration omitted
`experimental3DAllowed`, so the current product default for its next launch is
3D-off. The process and VM media were not stopped or mutated.

## Retained claim

Source `093da68729ee2458558396573c854e3e037dffd1` extends the additive v1 runtime
session observation with an optional `graphicsMode` field:

- an app-owned session reports `basic-3d-off` or `experimental-3d` from the
  frozen configuration retained for that exact launch;
- a pre-existing attached session reports `unverified` because the app did not
  launch it and cannot reconstruct its accepted arguments;
- terminal and unobserved sessions make no current graphics claim; and
- responses from older apps remain decodable when the optional field is absent.

Validation rejects an experimental or basic claim from an attached session and
rejects `unverified` for an owned session. Text output prints `not-reported` for
an older response instead of inferring from the current registration.

## Deterministic verification

Eleven focused Apple XCTest cases passed across retained owned observations,
attached observations, codec compatibility and stop targeting. The repository
Swift harness passed. The real-process native runtime contract then exposed an
explicit-source fixture omission for the new validator; source
`093da68729ee2458558396573c854e3e037dffd1` includes the corrected fixture and
all 22 socket, ownership, timeout and control contracts pass.

The source-head full project check ran all executable suites and failed only the
expected stale capability identity plus that now-corrected fixture omission.
Exact metadata-head local and hosted verification remain required.

This is status provenance, not a rendering repair. It does not prove a 3D-off
live frame, diagnose the sign-in glyph defect, complete the B6 matrix or promote
graphics capability. A11 and B6 remain OPEN, product state remains Engineering
Preview, and 3D remains outside General Preview and v1.
