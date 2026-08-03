# A1: why `firstboot_fresh` was stuck at 0, and what actually fixed it

Status: **A1 does not pass.** Over twenty boots the rate is **17/20 = 85%**,
below the 90% the criterion requires. A first gate returned 9/10 and was briefly
recorded here as a pass; a second gate returned 8/10 and that claim is
withdrawn.

What this document still establishes is that `firstboot_fresh` now *measures*
correctly — the instrumentation blocker described below was real and is fixed.
The remaining obstacle is a different one: an intermittent boot stall.

## The claim that was wrong

An earlier handoff recorded that `firstboot_fresh=1` was "unattainable on the
current image" and that closing A1 needed a fresh install, and therefore the
owner's Windows ISO. Both halves were wrong, and no ISO is required.

## What `firstboot_fresh` actually measures

`scripts/run-hvf-windows-installed-boot-runner.sh:1098` compares the SHA-256 of
the guest's `viogpu3d-firstboot.log` before and after the run:

```
if [[ "$pre_sha" != "unavailable" && "$current_sha" != "$pre_sha" ]]; then
  fresh=1
fi
```

It asks "did the firstboot log change during *this* boot", not "was Windows
freshly installed". The check exists because the log persists on the guest disk,
so an old `[stage4] done` would otherwise be read as a pass for a boot that
never ran firstboot at all.

## Why it read 0

Two independent reasons, both invisible without looking at the guest:

1. **The scheduled task was gone.** Queried live in the guest:

   ```
   BV-T| task_missing
   BV-F| stage1.flag exists=True
   BV-F| stage2.flag exists=True
   BV-F| stage3.flag exists=True
   BV-L| log_mtime=2026-07-31T00:35:01Z bytes=11400
   BV-L| now_utc=2026-08-02T15:08:40Z
   ```

   `BridgeVM-VioGpu3DFirstBoot` no longer exists, so nothing has run firstboot
   since 2026-07-31. The log has not been written for two days.

2. **The stage flags short-circuit the script.** `bvgpu-firstboot.cmd:38-41`
   jumps straight to `:stage4` once `stage1..3.flag` exist, and stage4 is gated
   on `require_new_boot`. On a finished image there is nothing left to do.

The log the gate was reading is also from a *different driver*: it names
`viogpu3d.inf_arm64_44e90b7a44a1d335`, the shipped 120.41 package, while the
image under test loads `viogpu3d.inf_arm64_6435ce2e01767d8f`, our fixed build.
The recorded `[stage4] Vulkan probe errorlevel=13` is a two-day-old failure
against superseded code — exactly the stale-evidence trap the freshness rule was
written to catch. The rule worked.

## The fix

None of this needed new code. `scripts/p1-boot-gate.sh` already boots a fresh
APFS clone plus the injector on every iteration, so firstboot runs from stage1
each time and the log is necessarily rewritten. Its header says so, and it also
refuses to run against an injector older than `scripts/win-assets`.

That staleness guard fired here and was correct: the injector on disk predated
the A8 work. Rebuilding it against the fixed driver package was the only action
required.

## Result: 17/20, which is below the bar

Two independent ten-boot gates, each boot a fresh clone plus injector:

| gate | pass | note |
| --- | --- | --- |
| `a1-gate10` | 9/10 | meets the bar exactly |
| `a1-gate10b` | 8/10 | below the bar |
| **combined** | **17/20 = 85%** | **below the 90% required** |

A single 9/10 sits exactly on the threshold, so it is the weakest possible
evidence of a pass — one more failure in either direction flips it. Running a
second gate was the right call: it did flip.

Verified per boot rather than trusting the summary. Reading each
`firstboot-stage.txt` directly:

| boot | stage4_pass | firstboot_fresh |
| --- | --- | --- |
| 1, 3, 4, 5, 6, 7, 8, 9, 10 | 1 | 1 |
| **2** | **0** | 1 |

All ten boots wrote a fresh log, so the freshness half is unanimous. The gate's
own `progress.txt` recorded boot 2 correctly as
`stage4_pass=0 fresh=1 last_stage=stage3 stalls=1`; an intermediate `tail` of
the file during the run was misread as showing all ten passing, which the
per-boot check corrected.

Sampled passing boots reached the real end of the script:

```
[stage4] DXVK D3D11 present errorlevel=0
[stage4] done Sun 08/02/2026 15:22:53.68     (boot-1)
[stage4] done Sun 08/02/2026 16:19:55.45     (boot-5)
[stage4] done Sun 08/02/2026 16:42:02.65     (boot-10)
```

### Every failure is the same boot stall, at a different stage

All three failures across the twenty boots share one signature — the guest is
not executing at all:

| run | last stage | signature |
| --- | --- | --- |
| `a1-gate10/boot-2` | stage3 | `exits_in_window=0 suspect=guest-not-running` |
| `a1-gate10b/boot-1` | stage1 | same |
| `a1-gate10b/boot-9` | none | same |

They stall at three different points, so this is not a defect in any one stage.
Taking `a1-gate10/boot-2` as the worked example, it stopped after stage3's
reboot with `last_stage_observed=stage3`:

```
probe: boot-progress watchdog stalled_for_ms=120000 exits_in_window=0
       total_exits=412803 reboots=3 suspect=guest-not-running
```

`exits_in_window=0` means the guest was not executing at all — the intermittent
boot stall tracked separately, not a defect in the graphics path. Its stage4
probe never ran, so it is scored as a failure, which is the conservative
reading.

Its last captured frame is entirely black and it never reached the 5 s ramfb
checkpoint, consistent with a guest that never started.

**A hypothesis tested and rejected:** in the first gate the failing boot was the
only one reporting `psci 8` where passing boots reported 27, and the BAR2
`base_changes` counter stopped at 19 against 23 for every pass. That looked like
a clean split. The second gate refuted it — `a1-gate10b/boot-9` failed with
`psci 26` and `base_changes=7`. PSCI count is not the discriminator.

**A1 therefore stays open, and it is now blocked on the boot stall rather than
on measurement.**

## Note on the owner decision recorded in GOAL.md

A1 carries a "⚠ owner decision needed (D2)" note asking whether A1 should be
measured by probe success or by whether the guest is actually usable. This
finding does not settle that question, but it does remove the reason to weaken
the criterion: the strict rule is satisfiable as written.
