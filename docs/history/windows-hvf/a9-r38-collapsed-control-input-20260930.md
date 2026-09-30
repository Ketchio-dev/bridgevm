# A9 r38 collapsed control input — failed physical diagnostic

Classification: one failed, nonpromoting physical diagnostic. The current A9
criterion, defect wording and product state come from
`capabilities/windows-hvf.json`; A9 remains OPEN and the product remains
ENGINEERING_PREVIEW.

## Sealed observation

Physical job `t17-345bfd9a-ax-deadline-pilot-r38` ran
`345bfd9a8d59ebcdfc1acc6f0a917b177c1d2fc7`, the head of PR #275, which merged
unchanged into main. All 95 hosted runs for that SHA succeeded, including CI
`36655376914`, Security and quality `36655376823` and the full project check
`36655375282`; the local full project check also passed. It carried the
[r37](a9-r37-first-ready-20260930.md) lookup-deadline correction. The app was
a local exact-source Apple Development build; the Accessibility preflight and
console check passed, and the input manifest SHA-256 was
`92417ebd2844b20315b7616770511e4f108decff76d1c5056cef75b4a062d1ff`.

The job ran on Mac17,9, macOS 27.0, from 01:54:02 to 02:08:29 UTC and failed
with exit status 1. Public and private receipts were byte-identical at SHA-256
`f2e92aea0e37ac34e5b1b536994615804355ec38b2cced8afd812d973c68c487`; both
official validators and `bridgevm-live receipt` passed. Worker cleanup was
verified, and sixteen once-a-minute console samples found the session
unlocked throughout.

Artifact preflight, VM creation, source preparation, installation, Secure Boot
provisioning and first READY each passed once, as in r37. The authenticated
lane result (SHA-256
`876f8c347391432bb83ce179979f0b80220d34922a80c58fdc1ffb5d5f1f897c`) then
recorded
`ax_tree_read_failed;attribute=AXIdentifier;role=AXMenuButton;ax_error=-25200;stage=identifier-search;identifier=bridgevm.runtime.ctl.input`.

## Reading and correction

Because the lookup now polls until its unchanged 10-second deadline, this
read failure was present for the whole search rather than momentarily. The
role-qualified search keeps a read failure only when no readable node matches,
so the guest control input itself was not found at any point.

The input and its send button have lived inside the runtime view's collapsed
"advanced diagnostics" disclosure group since `400af559` (2026-09-15). The
T17 journey was written on 2026-09-01 and never opens that group, and SwiftUI
leaves collapsed content out of the accessibility tree. No pilot reached this
step before r37, so the mismatch was not observed until now. The r37 reading
that a transient read error had ended the lookup is superseded: the lookup
fix was still needed, but the input was absent, and the reported read failure
came from an unrelated menu button.

The group's label is now a button with a stable identifier. The journey enters
each control command through one helper that presses it only when the input is
absent, so an open group is never toggled closed. A contract requires any
collapsed identifier that T17 uses to go through that opener; it failed on the
previous tree.

## Limit

This is a second single run, not a rate. No journey stage after first READY
passed, and the correction has not run on hardware. A9 and A11 remain OPEN and
product state stays ENGINEERING_PREVIEW.
