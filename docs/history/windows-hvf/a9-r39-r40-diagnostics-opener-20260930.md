# A9 r39 and r40 diagnostics opener — failed physical diagnostics

Classification: two failed, nonpromoting physical diagnostics. The current A9
criterion, defect wording and product state come from
`capabilities/windows-hvf.json`; A9 remains OPEN and the product remains
ENGINEERING_PREVIEW.

## Shared inputs

Both jobs ran `55a1ed1c4d5a2e4ab283c72f87b5c17cf87c4dfa`, the head of PR #276,
which merged unchanged into main. All 95 hosted runs for that SHA succeeded,
including CI `36659135598`, Security and quality `36659135606` and the full
project check `36659133141`; the local full project check also passed. They used one local
exact-source Apple Development build and one input manifest, SHA-256
`5e12c17af5e6c120d02d1ee3d99131dfeccc4c6c149c01c7c57b01e17d19de9b`. The
Accessibility preflight and console check passed at each submission. The
build carried the [r38](a9-r38-collapsed-control-input-20260930.md) opener
for the collapsed runtime diagnostics. Background-priority builds and tests of
unrelated branches ran on the host during both jobs.

## r39: focus taken by the operator terminal

Job `t17-55a1ed1c-diagnostics-opener-pilot-r39` ran from 03:07:56 to 03:15:56
UTC. Byte-identical receipts (SHA-256
`7fa0899fb4e9000f32fac354305fc69da71b798bad3f88fafeca5e363e2450ad`) and the
lane result (SHA-256
`84f72152b5ef6bd682050dcf8cfda8dc3b74f3fdea3d57d68d6dff3f8da87ca0`)
validated. VM creation, installation and Secure Boot provisioning passed. The
lane then failed `input-selection-failed` in the runtime host-share chooser
before Start: the Open button never became enabled. At the timeout the product
app was not active and the frontmost process was Ghostty, the terminal hosting
the agent session, into which operator messages were typed during the run.
This run therefore says nothing about the product or the opener.

## r40: opener exposed as a disclosure triangle

Job `t17-55a1ed1c-diagnostics-opener-pilot-r40` ran from 03:18:06 to 03:32:16
UTC with thirty half-minute console samples all unlocked. Byte-identical
receipts (SHA-256
`0aa6ba839bea844b8a19d1fbb789b25ca43482f08ad44199928f432cf04ec95c`) and the
lane result (SHA-256
`5920442047d0414e98c0521db8a0379c2069a62c711a03c346582948b8b50bfe`)
validated. Installation, Secure Boot provisioning and first READY each passed
once, the third first READY in a row since the provisioner correction. The
lane then failed looking up `bridgevm.runtime.diagnostics.toggle` as an
AXButton.

A local probe rendered the same SwiftUI shape and walked it through the
Accessibility API as T17 does. A button inside a `DisclosureGroup` label was
exposed as an `AXDisclosureTriangle` carrying the identifier, and `AXPress` on
it returned success and exposed the collapsed text field; a plain button was
exposed as `AXButton`. T17 now has an explicit `expand()` that looks up the
disclosure-triangle role and shares the existing press policy, and the opener
uses it. A contract that fails on the previous tree pins both.

## Limit

These are single runs. No journey stage after first READY passed, and the
`expand()` correction has not run on hardware. A9 and A11 remain OPEN and
product state stays ENGINEERING_PREVIEW.
