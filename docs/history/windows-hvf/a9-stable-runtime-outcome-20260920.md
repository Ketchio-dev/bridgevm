# A9 stable runtime outcome — 2026-09-20

This checkpoint does not close A9. Product state remains Engineering Preview,
A9 and A11 remain OPEN, and 3D remains outside General Preview and v1.

## Measured failure

Exact-main physical job `t17-8d4bbaed-first-ready-terminal-r18` passed artifact
preflight, created the VM, prepared the source, installed Windows and
provisioned Microsoft-only Secure Boot. Installation finalized transactionally:
the final disk, variables and Secure Boot receipt were published and no install
journal remained.

The first-ready observer then failed closed. It requested both
`bridgevm.windows.runtime.state` and the optional
`bridgevm.windows.runtime.start.failure` from one complete Accessibility graph.
The failure identifier was absent, while an unrelated SwiftUI element returned
generic AX error `-25200`. Five complete reacquisitions could not prove whether
the optional element was cleanly absent. The authenticated private lane result
reported `first_ready=false`, `failure_code=ui-element-missing` and verified its
own cleanup. The outer wrapper separately reported cleanup failure because its
temporary root survived; a later bounded ownership check accepted that exact
tree and removed it. Neither result proves first READY.

During the same 3D-off pilot, retained WinPE frames displayed DiskPart and DISM
text normally and installation reached completion. That is useful contrast with
the historical experimental-3D missing-glyph frame, but it is not a B6 matrix
result and does not close the graphics defect.

## Deterministic change

Source `2aebbc4f9bf1929a610500faad189cd02572e389` keeps the exact startup-outcome
Accessibility element present in both normal and failure states. Its normal
value is the stable token `none`; a real startup error retains its existing
nonempty value. The product observer normalizes only exact `none` to no failure.
All graph acquisition, exact-identifier, ambiguity, role and AX error rules are
unchanged.

Fourteen focused first-ready and optional-projection contracts passed. The
complete source-head project check passed every other executable, app, security,
documentation and structural step and correctly failed only stale capability
identity before this record. It reported shim suites of 425, 856 with two
required live-only skips, 62 and 123 tests. Its retained 7,717-line log SHA-256
is `e41cb2fc5dafc30cd54616411b27d78e677100772ead92c3ec2f61efa88d2dc0`.
PR 233 source-head hosted CI likewise retained the expected stale capability
identity failure before this metadata checkpoint.

## Limits

This change requires exact metadata-head hosted verification and another signed
physical T17 pilot. It does not prove READY/PONG, an installed Windows desktop,
the complete integration journey, regression closure, release readiness or a
graphics repair. No criterion, threshold, product state or sample count changes.
