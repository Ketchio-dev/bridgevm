# A9 runtime share chooser — 2026-09-21

Historical evidence record. The capability registry owns current wording and
status. This checkpoint does not close A9 or A11, product state remains
Engineering Preview, and 3D remains outside General Preview and v1.

## Physical r22 result

Exact-main commit `66366e97510de64887409b369150d012342e2281` was
packaged as an Apple Development-signed, 3D-off artifact and used by physical
pilot `t17-66366e97-start-admission-r22`.

The pilot passed artifact preflight, exact VM creation, source preparation,
Windows installation and Microsoft-only Secure Boot provisioning. The live
installer reached DISM apply and retained `bcdboot=complete`; the finalized
64 GiB disk and 64 MiB variables were authenticated. After installation the
exact runtime Start control remained disabled for its complete 20-second bound,
so no runtime process or run log was created and READY/PONG was not observed.

The authenticated lane retained `failure_code=ui-element-missing` with the
bounded detail `identified UI element remained disabled` and verified cleanup.
The outer public taxonomy remained `product-model-failed`, with
`criterion_pass=false`, `claim_eligible=false`, one Windows-install pass, one
Secure-Boot pass and zero first-ready passes. 3D injection remained false.

The strict public receipt passed repository verification and has SHA-256
`57819231f685ce8b4ca6d074e5d11f2937c3c61262469bef7ab562fd5eab9633`.
The authenticated private lane result has SHA-256
`bdd92df00aa81d8b471e5a2f33e9ab81a4f88c4eddfea0d40a123a0f8a811cfa`.
Private paths, media and guest state remain outside git.

## Conclusion

r22 proved the new enabled-control admission behaved truthfully: it did not
turn a successful AXPress return on a disabled button into a boot attempt. It
also disproved the prior working assumption that an exact AX text field plus
AXConfirm was sufficient to update the SwiftUI launch configuration. The
product model still did not admit Start.

The host share is a directory selection, and the visible product already owns
an NSOpenPanel path for it. Source
`eda9ba6bfd958f67cefd060ee3cd109ab5af46f4` gives that folder button an exact
Accessibility identifier and makes both ISO-install and installed-disk-import
journeys choose the sealed directory through the real picker. The helper no
longer injects host or guest paths as text. It reads back the existing product
guest-path default and requires exact `C:\bridgevm-share`, failing closed if the
product contract changes.

Three focused contracts cover the exact toggle and chooser sequence, chooser
failure propagation and changed-default refusal. All 133 ProductE2E XCTest
contracts, all 41 Swift Testing contracts and structural budgets passed. The
complete local project check also passed on the source-plus-registry tree; its
retained 7,901-line log has SHA-256
`c684209f9cb2b20028ded7a4fcb56303df57417e8fe82b8670c9e1301d12c349`.

## Limits

The physical r22 pilot is a failed single run and does not prove guest boot,
READY/PONG, integration behavior, clean-machine completion or A9. The chooser
correction is deterministic host evidence only until a newly signed exact-source
artifact passes another physical pilot. Even a successful replacement pilot is
a live single run rather than the release sample count. T19 remains required,
and no threshold, timeout or capability wording was relaxed.
