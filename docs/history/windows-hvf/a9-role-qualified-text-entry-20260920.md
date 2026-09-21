# A9 role-qualified runtime text entry — 2026-09-20

This checkpoint does not close A9. Product state remains Engineering Preview,
A9 and A11 remain OPEN, and 3D remains outside General Preview and v1.

## Physical r19 result

Exact-main commit `af66b9a7f619b09ee6fa8bbd5df4f6d3d1b4f89b` completed all
45 hosted push workflows. Its Apple Development-signed artifact passed deep
signature, entitlement, nested-helper, swtpm, wimlib, notice and LaunchServices
Accessibility verification before physical job
`t17-af66b9a7-stable-runtime-r19` was submitted.

The 3D-off pilot passed artifact preflight, exact VM creation, source
preparation, Windows installation and Microsoft-only Secure Boot provisioning.
Installation finalized with `installPending=false`, a final disk, UEFI variables
and a Secure Boot receipt. Retained WinPE frames rendered DiskPart and DISM text
normally; this is useful contrast with the historical experimental-3D glyph
defect, but it is not a B6 matrix result.

First boot never launched. A bounded Accessibility snapshot during the
first-ready wait reported the Start button disabled, runtime state `stopped`,
startup failure `none`, all three required runtime toggles enabled, and an empty
host-share field. The exact identifier `bridgevm.runtime.share.host` appeared on
an `AXStaticText`, one `AXTextField` and an `AXButton`. Identifier-only text
entry had selected a noneditable semantic sibling and returned without placing
the required host path in the field. No runner child or `run.log` existed.

The authenticated lane failed with `failure_code=guest-evidence-missing` and
`first boot has no BVAGENT READY/PONG evidence; run_log=status=absent`. It kept
`first_ready=false`, verified cleanup, removed its private temporary root and
left no matching process. Private and public receipts both passed the strict
verifier. The lane result SHA-256 is
`a983e4bd3f177304e749d558fed5f629b710ad279801e19f089bc0b6616eabd9`;
both receipts have SHA-256
`7ae56c552d5d18e92c04fe002ff0a98a6a1b29bcd00b6467b7edbc61342e7223`.

## Deterministic correction

Source `22c3f881f055583eb1c6e9a40a820c477f1fed8a` resolves text-entry
targets by exact Accessibility identifier and exact `AXTextField` role in one
bounded application graph. Missing and duplicate exact identities fail closed.
An exact-identifier role read failure propagates even if another readable field
exists; unrelated identifier read failures retain the existing coherent-snapshot
behavior and cannot replace an exact readable match.

Six focused role-identity contracts passed. The complete source-tree
`scripts/check-project.sh` passed every executable, app, security,
documentation and structural gate; its retained log SHA-256 is
`78565fc49627f3413275cbd8b66d68410f862eca62c2d8615a7ce58ce5c7aa60`.
PR 234 source-head CI passed structural budgets and many independent jobs, then
correctly rejected the stale capability identity before this record.

## Limits

A new signed physical T17 pilot is required. One pilot would still be useful
signal rather than the required release sample count, and T19 remains required.
This checkpoint proves no READY/PONG, usable Windows desktop, complete
integration journey, regression closure, release readiness, performance or
graphics capability. No criterion, threshold, timeout or sample count changed.
