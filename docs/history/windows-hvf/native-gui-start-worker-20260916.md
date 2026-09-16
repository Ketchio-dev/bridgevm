# Native GUI startup worker — 2026-09-16

Classification: historical deterministic evidence. Product state remains in the
[registry](../../../capabilities/windows-hvf.json). A11 stays OPEN and
ENGINEERING_PREVIEW is unchanged. This follows the [CLI startup record](native-owned-start-deterministic-20260916.md).

## Implemented boundary

Ordinary GUI Start now reserves the retained session before publishing accepted
editor configuration, then performs process discovery, readiness, preparation,
interactive key resolution and process launch on a detached worker. GUI policy
continues to accept edited configuration, attach to an observed external runtime,
and use the compatibility wrapper when that was the available GUI route. Empty
TPM state may create a key; populated state may not silently replace its key.
No host key is read by the deterministic fixtures.

The GUI ticket has no fabricated saved-config digest and no new arbitrary
interactive-prompt timeout. The separate CLI path retains its exact saved-config
binding, existing-key-only lookup, typed runner requirement and 30-second bound.
A pending GUI worker keeps the session active and blocks competing GUI/CLI starts,
TPM recovery/reset, snapshots and library/storage actions. Removed library entries
remain retained while active. The worker rechecks admission before preparation/key/launch.

A spawned result is always adopted, even if admission changed while launch was
finishing. Invalidated typed processes use the owned supervisor stop protocol;
legacy processes retain their exact Process until observed exit. Cleanup latches
an observation-only mode before publishing state or cancelling input: polling
observes only the retained controller or child and refuses control/input writes.
It never follows changed evidence/control paths during that stale cleanup.
Status publications are revalidated before guest input/normal polling begins.
External attachment remains observation, and invalidating it never stops that VM.

Runtime readiness display uses one retained detached reader. Repeated edits
coalesce to the latest configuration/repository; superseded results are discarded.
Unknown or pending reports disable Start, and refresh explicitly invalidates the
cached report. The launch worker still performs authoritative readiness checks.
Preparing progress and selectable failure detail appear beside runtime controls;
pre-spawn Stop is disabled, and superseded errors clear on a later start.

## Review and deterministic validation

Independent review found two reachable integration defects before validation:
publication callbacks could invalidate configuration after a cached admission
check, and input cancellation could poll changed paths during late legacy cleanup.
Both were corrected; the latter has a replacement-log/control-sentinel regression.
Review also identified synchronous readiness in render and stale error feedback;
these were corrected with asynchronous reports and explicit supersession.

| Check | Result and provenance |
| --- | --- |
| Initial combined control shim | FAIL at test compilation in 14.339 seconds; fixture referenced record.isActive instead of the existing descriptor.isActive; source inputs remained unchanged |
| Corrected combined control shim R2 | PASS: 740 passed, zero failed, two required live-only skips in 72.907 seconds; all inputs unchanged; all 21 new GUI/readiness tests executed |
| Focused readiness notification regression | PASS: refreshed product module and all five model tests in 13.637 seconds, stable inputs; synthesized observation removes the new isolation warning while preserving reentry behavior |
| Frozen full deterministic project check | PASS in 218.684 seconds at source bcb6708a235f41bd8a2cef7ebe8cd3fb5e795a89; all 3,206 tracked inputs, HEAD and index unchanged; log SHA256 `219d0bc56a60958be1cd93a0a454d44f84411558aa2940a07329cd7766b3e7f3` |
| Exact new-head hosted checks | Pending; no release evidence promoted |

The new regression cases cover editor configuration, publication reentry,
reservation conflicts, removed-library retention, off-main interactive-key and
readiness waits, single-reader coalescing, key policy on both launch routes,
external attachment, late results and actual harmless runner/child cleanup.
Synthetic helpers, injected keys and independently observed child exits establish
process contracts; they do not establish Windows guest or normal App.main behavior.

## Repository integration and remaining evidence

PR161 exact e5024e1b05d8e7d4629720bbd50b42d754619b74 completed 85 hosted workflows
successfully (120 successful checks and one skip). It was normally merged as
756405a58f43704668abaf12bd057b57e384da11 at 07:18:26 UTC. The merge tree matches its
verified PR head; postmerge checks were 35 successful and 12 pending at the last
bounded observation. Stop/cancellation branches remain until that main check and
branch/worktree retirement audit complete.

CLI startup 9ba394ec5d7541749d9890962d5eb543de5e1404 completed 40/40 hosted workflows.
Integrating main at 00133d3cc973b039f5dcd9a4615b1fcb5f1f5ff1 preserved its exact
tracked tree; PR162 was opened for normal integration and awaits its own checks.
PR159/160 main verification and branch retirement were already completed.

Actual native UI pilot18 remains Accessibility-untrusted: 0/7 actions, 4/8 captures.
No permission-state change, new physical job or native UI success is claimed here.
Live app/guest startup/stop, guest flush, supervisor crash recovery, platform
Keychain behavior and manual accessibility remain unproven. All 29 capability
criteria, thresholds, blocking flags and product wording remain unchanged.
