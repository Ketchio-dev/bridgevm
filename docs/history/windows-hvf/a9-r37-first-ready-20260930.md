# A9 r37 first READY — failed physical diagnostic

Classification: one failed, nonpromoting physical diagnostic. The current A9
criterion, defect wording and product state come from
`capabilities/windows-hvf.json`; A9 remains OPEN and the product remains
ENGINEERING_PREVIEW.

## Sealed observation

Physical job `t17-6da325c7-provisioner-fix-pilot-r37` ran exact main
`6da325c76fece363697ffbd5422d91e37e25970b`. Its 50 main-push hosted workflows
succeeded, including CI `36647840238`, Security and quality `36647840646`
and the full project check `36647846786`. That main carries the
[first-logon provisioner correction](a9-firstboot-provisioner-and-shutdown-gate-20260929.md)
and the unlocked-console submission check. The app was a local exact-source
Apple Development build, the LaunchServices Accessibility preflight and the
console check passed at submission, and the input manifest SHA-256 was
`a992efb3d555edbe3d8b6272e94da7d906b24c8fdd2d8f7fd4a96c675ca03e11`.

The job ran on Mac17,9, macOS 27.0, from 00:27:01 to 00:40:42 UTC and failed
with exit status 1. Public and private receipts were byte-identical at SHA-256
`edcf745f33d2e29b2b4bf4b118f0c9f614167f3ee5bc284ab1a4700e7fcbcb2d`; both
official validators and `bridgevm-live receipt` passed. Worker cleanup was
verified. Sixteen console samples taken once a minute from submission to
completion all found this user's session on screen and unlocked.

## Result

Artifact preflight, VM creation, source preparation, Windows installation,
Secure Boot provisioning and first READY each passed once:
`first_ready_passes=1`. This is the first T17 run in which a guest installed
through the packaged app reported BVAGENT READY. r32, r33 and r35, which ran
the same install path before the provisioner correction, each reached a
running guest without READY.

The authenticated lane result SHA-256 was
`6ff174f38c9d35cf139cb7072947cc5899c7e02dbbb32035251bacfc32a68434`. The
first journey step then failed while entering the first guest control command:
`ax_tree_read_failed;attribute=AXIdentifier;ax_error=-25200;stage=identifier-search;identifier=bridgevm.runtime.ctl.input`.
No keyboard, clipboard, share, network, audio, shutdown or snapshot stage ran.
Because the lane failed after first READY with a UI error, no first-READY
diagnostic packet was collected; only the receipts and lane result remain.

## Reading

One run passing first READY after three failures on the same path is
consistent with the provisioner defect having blocked the agent, but it is a
single observation, not a rate. The new failure is a generic Accessibility
read error during an identifier search. Code reading shows that an identifier
lookup ends as soon as one snapshot exhausts its three quick retries for such
an error, even when the lookup's own timeout has not passed; that is a
hypothesis about this failure, not an observed cause.

## Limit

No journey stage after first READY passed, so no desktop integration,
shutdown, snapshot, clean-machine or import behaviour is shown. The receipt
keeps `criterion_pass=false` and `capability_promotion=false`. A9 and A11
remain OPEN and product state stays ENGINEERING_PREVIEW.
