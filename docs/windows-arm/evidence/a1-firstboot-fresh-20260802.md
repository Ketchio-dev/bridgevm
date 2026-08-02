# A1: why `firstboot_fresh` was stuck at 0, and what actually fixed it

Status: measuring. The blocker is identified and removed; a 10-boot gate is
running to produce the pass count A1 requires.

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

## Result so far

A two-boot trial with the rebuilt injector:

```
boot 1   injected=true  stage4_pass=1   fresh=1   last_stage=stage4   stalls=00  reboots=3
boot 2   injected=true  stage4_pass=1   fresh=1   last_stage=stage4   stalls=00  reboots=3
pass=2  rule=stage4_pass==1 AND firstboot_fresh==1
```

Both boots satisfy A1's rule unmodified. The full ten-boot gate is in progress;
A1 stays open until it reports.

## Note on the owner decision recorded in GOAL.md

A1 carries a "⚠ owner decision needed (D2)" note asking whether A1 should be
measured by probe success or by whether the guest is actually usable. This
finding does not settle that question, but it does remove the reason to weaken
the criterion: the strict rule is satisfiable as written.
