# Install finalization worker — 2026-09-20

This is a deterministic responsiveness checkpoint, not A9 completion or a
release-readiness claim. A9 and A11 remain OPEN, product state remains
Engineering Preview, and 3D remains outside General Preview and v1.

## Physical observation

Exact-main physical job `t17-26c56ce2-first-ready-terminal-r17` passed all ten
artifact inputs, VM creation, source preparation, Windows installation and
Microsoft-only Secure Boot provisioning. It did not produce first-READY
evidence. The terminal observer failed closed when reading the optional startup
failure identifier returned AX error `-25200`; cleanup was verified. The strict
public receipt passed the repository verifier and has SHA-256
`b3b1c6715db81cc6f3abaf6fe8dfc1ddc052150e8e6a6ed50e733afd94fa4658`.

While that job finalized its 16 GiB installed disk, a process sample placed the
app's main thread in `HvfWindowsInstallFinalization.resume`,
`HvfWindowsStableFileDigest.compute` and the SHA-256 compression routine. The
captured coherent framebuffer from the separate pre-existing experimental 3D
session still showed the known guest composition defect; that session was not
stopped or changed.

## Deterministic change

Source `5cbfa760778be77168f5a3e1fdbc6b5b6abc41a5` runs the existing journaled,
fail-closed install finalizer in a detached user-initiated worker and awaits its
actual result from the owning session. The durable commit boundary and ordering
are unchanged. Once finalization is admitted, cancellation of the awaiting task
does not abandon it; success and the original error still return exactly once.

Three focused worker tests prove off-main execution, unchanged error
propagation and completion after awaiter cancellation. Forty-three install
finalization, integrity, cancellation and recovery tests pass together.
Structural budgets pass with both new files registered at their actual sizes.

The complete source-head project check ran all deterministic sections. It
passed every executable, application, security, documentation and structural
check and failed only the expected stale `tested_commit` gate before this
record. The Swift shim suites reported 425, 856 with two required live-only
skips, 62 and 115 passes. The retained 7,714-line log SHA-256 is
`96e389c0f5cb7f91902dafbc25979cb64f11b5cc5decf1351caa48e3cb6c1630`.

## Limits

This result removes a measured host UI stall mechanism. It does not measure the
new wall-clock duration, prove a live responsive window, repair the first-READY
AX observation, prove an installed guest boot, or close any sample criterion.
Exact metadata-head local and hosted verification are still required.
