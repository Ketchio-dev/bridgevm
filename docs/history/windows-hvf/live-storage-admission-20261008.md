# Explicit worker storage admission — 2026-10-08

The queue installer accepted a custom queue but its sanitized LaunchAgent
argv dropped that setting. The worker could therefore use its HOME default
instead of the selected queue. The installer now renders plist data using an
isolated Python interpreter and explicitly freezes queue/work/minimum settings
inside the existing Bash-p/env-i argument vector. Parent HOME aliases remain
supported; leaf aliases and ambiguous terminal path spellings refuse. Printed
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

Focused fixtures: capacity7, renderer/custom-installer4, existing installer
launch3/dry11, worker venue2, policy103, T17 cleanup-fence5, T22 worker21 PASS.
The isolated renderer ignores owned PYTHONPATH startup code; its nonisolated
control executes the sentinel. Exact plist special characters and hostile
inherited storage overrides are covered. Tests use controlled providers, not
actual space exhaustion or Windows hardware. One targeted invocation named a
nonexistent file; the next T22 wrapper hit90s after17 dots and is interrupted,
not passing. Its unchanged rerun completed21 in95.129s; retained fixture residue
from the interruption remains distinct. Full local/current-SHA hosted checks
remain pending. Product state and all criteria are unchanged; A9/A11/A19 OPEN.
