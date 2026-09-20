# HVF guest window title identity

## Observed defect

An older development preview displayed an own-HVF Windows guest in a separate
macOS window titled `disks`. The app derived that title from the immediate
parent of `hvf-target.raw`, which is the storage-layout directory rather than a
VM identity.

This did not alter guest state, but it made multiple guest windows difficult to
distinguish and exposed an implementation directory in the user interface.

## Correction

Source `446d90aa04abd7d5a06314984f815b62c6632d7f` moves title selection into a
pure resolver with three bounded outcomes:

- a current library session uses its exact saved VM display name;
- a legacy session may recover the VM directory only from the exact
  `<vm>/bundle.vmbridge/disks/<disk>` product layout; and
- an empty, relative, arbitrary or near-matching path uses `Windows HVF`.

The fallback does not inspect the filesystem, infer a name from a generic parent
directory or change the session identity used for lifecycle control.

## Deterministic verification

Five focused Apple XCTest cases cover a saved name, the legacy product layout,
a whitespace-only saved name, arbitrary and relative paths, and a deceptive
`disks-extra` near-match. Structural budgets pass with both new files registered
at their actual 19 and 36 lines.

Before the main rebase, the complete source-head project check ran all executable, documentation and
structural suites. BridgeVMControl reported 848 shim passes with two required
live-only skips; the only failing step was the expected stale capability
identity before this record. The retained 7,851-line log SHA-256 is
`e27e610b8c5de0b1c3a42784e05265d48e588c3580d67031706c4dbde5ddc4bc`.

This proves deterministic title selection only. It does not prove a rendered
window, guest boot, glyph repair or graphics behavior. A9, A11 and B6 remain
OPEN, product state remains Engineering Preview, and 3D remains outside General
Preview and v1.
