# A1 stage4 after the BAR2 fix — 10/10 (2026-08-01)

`firstboot` stage4 is the Vulkan probe: `bvgpu-firstboot.cmd:128-150` runs
`bvgpu-vulkan-probe.ps1` with a 45-second bound and fails the stage on a
non-zero errorlevel. Before 2026-08-01 that probe hung inside
`vulkan_virtio.dll` and stage4 reported `errorlevel=13`.

## Result

Ten consecutive fresh cold boots of `canonical-attach-resident-20260731.raw`
(`cp -c` clone per boot, vars restored per boot), running the same probe with
the same 45-second bound that firstboot uses:

| run | boots | `create_instance_result` | `stage4_errorlevel` |
| --- | ---: | --- | --- |
| `a1-stage4-20260801-024428` | 5 | `0` in all five | `0` in all five |
| `a1-stage4-20260801-024959` | 5 | `0` in all five | `0` in all five |

10/10. The criterion is ≥9/10.

Driver provenance was asserted in-run on every boot:
`driverstore=viogpu3d.inf_arm64_6435ce2e01767d8f` (the ATTACH_BACKING-resident
build).

## Boot reliability

A separate ten-boot pilot (`a1-pilot-20260801-023516`,
`a1-pilot-20260801-023835`) reached `BVAGENT SERVICE start` in 10/10 with zero
`boot-progress watchdog stalled` events. The `reboots=1` figure in that
summary is a counting artefact: the matched line is the configuration banner
`PSCI SYSTEM_RESET max reboots: 8`, not a reset. Actual resets: none.

This does not yet close out the intermittent boot stall recorded in `STATUS.md`;
ten clean boots is consistent with the previous ~1-in-3 stall rate having been
caused by the same defect, but that is not established.

## What this does not prove

The A1 wording also requires `firstboot_fresh=1`. It cannot be satisfied on this
image: firstboot has already run to completion here, so `firstboot-stage.txt`
reports `firstboot_fresh=0` and `last_stage_observed=stage4` from the **July 31**
failure preserved in the snapshot. The stage4 gate itself was therefore
re-executed directly rather than inferred from a stale log.

Closing A1 on its own terms needs a fresh install-to-firstboot image on the
fixed build.
