# A9: stale Accessibility code requirement (2026-09-11)

Status: diagnosed denial; post-reset recovery is NOT yet proven.

## Sealed observations

Pilot `t17-65468519-grant-confirmed-r2` used commit
`65468519b385f06a9a90e711442e9df4b86e1715` after the user confirmed enabling
the exact packaged helper. It ended at `2026-09-11T21:49:29Z` with
`accessibility-untrusted`, before VM creation or Windows installation.
The private lane reported artifact preflight and cleanup successful.
This is a failed pilot, not an A9 criterion pass.

The caller was `dev.bridgevm.product-e2e`, PID 20949, PPID 1.
Its canonical bundle-path hash was
`ada2d600643b9b22201db127bf152af6f2961a162742c4473f3fd55fc4c20dd6`.

| Observation | Value |
| --- | --- |
| Current helper designated requirement | `cdhash H"dc85a0414ec157cd8741f7f115cd0b16a52c4bf8"` |
| Requirement shown in TCC mismatch log | `cdhash H"3243651768dbd73cbd193a61caef530f131544df"` |
| TCC static requirement-check result | `-67050` |
| Current helper strict signature verification | Valid on disk; satisfies its designated requirement |

At 17:48:53-54 EDT, the macOS TCC attribution records named the helper itself
as the responsible process. They repeatedly reported the different code
requirement and failed `SecStaticCodeCheckValidity`.
The local `security error -67050` diagnostic identified a failure to satisfy
the specified code requirement.

This supports a stale requirement mismatch for this denial. It does not prove
that every earlier denial had the same cause, or that launch-chain attribution
never matters. Preserve the [earlier attribution investigation](a9-accessibility-attribution-and-e2e-library-20260909.md)
as historical observations; do not use its explanation as a substitute for this
run's TCC evidence. PPID 1 alone is not a TCC responsibility diagnosis.

## Recovery boundary

The official command `tccutil reset Accessibility dev.bridgevm.product-e2e`
succeeded. Only this helper identifier was targeted; no TCC database was edited.
The same unchanged helper was selected in System Settings for re-registration.
At the last recorded handoff, the Open button was enabled but had not been
pressed by the operator. No post-reset pilot result exists in this note.

For this ad-hoc artifact, re-register the exact helper after resetting stale
records. A switch being on, a same-named application entry, or a successful
direct-shell probe does not prove trust in the actual queued launch context.
Rebuilding may change the code requirement and invalidate that registration.
Do not replace the binary in place while investigating a sealed run.

After registration, use a new pilot ID with the unchanged manifest and compare
the actual caller identity and failure detail. Do not retry identical failed
conditions, lower sample counts, or promote a single pilot to criterion success.
Capability and product-state wording remains owned by
[`capabilities/windows-hvf.json`](../../../capabilities/windows-hvf.json).

## Evidence retention

Private lane receipt:
`live-queue/done/t17-65468519-grant-confirmed-r2/private/lane-1-result.json`.
Scoped TCC log: `t17-grant-confirmed-r2-tcc-requirement.log` in the private
24-hour work ledger directory. It covers 17:48:50-17:48:56 EDT and only this
helper's requirement-mismatch records. Private host paths, credentials, guest
disks and unrelated system logs are not included in this document.
