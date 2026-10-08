# Explicit worker storage admission — 2026-10-08

The queue installer accepted a custom queue but its sanitized LaunchAgent
argv dropped that setting. The worker could therefore use its HOME default
instead of the selected queue. The installer now renders plist data using an
isolated Python interpreter and explicitly freezes queue/work/minimum settings
inside the existing Bash-p/env-i argument vector. Parent HOME aliases remain
supported; private owned queue-root aliases canonicalize, work aliases refuse. Printed
submit/status commands include the same shell-escaped queue selection.

The worker previously measured HOME free space, not its actual queue output
and Git/build work volumes. A failed/malformed df result could also continue
inside conditional run_job despite errexit. The new explicit refusal boundary
measures available bytes using descriptor fstatvfs, once per distinct device,
and requires the minimum across devices to meet the unchanged100GiB default.
Thresholds are canonical integers0..4096; existing zero-threshold synthetic
fixtures still probe providers and cannot bypass missing/read-only/error states.

Worker path validation precedes queue lock creation. Resolved /Volumes/name
storage must be a current mount; a missing mount is not replaced by an ancestor
filesystem. Work preparation creates only the final leaf after validation,
refusing missing parents. The installer can provision the validated hierarchy
as an explicit installation operation; dry-run never creates it and reports
only a provisional ancestor capacity estimate. Low capacity warns at install,
while invalid configuration or provider failure refuses. Job admission records
low reserve separately from unavailable storage and stops before Git or tiers.

This is a pre-start direct-filesystem reserve, not workload sizing, an atomic
capacity reservation, or proof that sparse-image backing storage can grow.
Image-backed pilot admission must separately authenticate the mount-to-backing
association and backing capacity. No automatic discovery or second queue is
introduced; final queue/work paths and any sole-queue migration remain guarded
operator operations. No installer, service, fence, TCC or media mutation ran.
Focused capacity7, renderer/custom-install4, launch3/dry11, venue2, policy103,
T17fence5 and T22worker21 PASS. Isolated renderer ignores owned PYTHONPATH
startup code; its nonisolated control executes the sentinel. Special characters
and hostile inherited overrides are covered; these are controlled providers,
not actual exhaustion or Windows. A mistyped invocation ran no test; next T22
hit90s after17 dots, interrupted not passing. Unchanged r3 completed21 in95.129s;
interrupted fixture residue retained. Full local/current-SHA hosted pending.
Queue-alias compatibility preserves existing V2 flow: actual worker/installer2
pass; unsafe755 targets refuse unchanged. Provider exit2 initially failed the
expected installer refusal1; normalized preflight retains that failed policy run.
Product state and all criteria unchanged; A9/A11/A19 OPEN.
