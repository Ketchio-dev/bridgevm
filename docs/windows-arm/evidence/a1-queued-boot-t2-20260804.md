# A1: the first boot gate to run from the queue (2026-08-04)

## What this run is

t2-pilot, 2 boots, job `20260804-210538-24736-10417`, on `bd28a03`. It is the
first boot gate that has ever completed from the local queue rather than by
hand. Getting there took eight fixes to the queue itself, each of which had
made every queued boot gate fail before it ever started a VM:

- the injector default pointed into `/tmp`, which is empty after a reboot
- freshness was keyed on mtime, and a `git worktree` restamps every file
- the asset digest was locale-dependent (ko_KR interactively, C under launchd)
- `cargo` was not on the LaunchAgent's PATH
- cancelling a job did not stop it, so one cancelled gate blocked the queue
- working directories were removed only on the success path, and 79 GiB of
  debris then tripped the free-space guard
- every tier ran 10 boots, so a pilot was indistinguishable from a campaign
- `injector_boot_observed` was compared against `1`; the runner writes `true`

The last one is worth stating plainly: it discarded a *successful* injection,
one whose guest logs show the agent installed and the D3DKMT probe returning
`0x00000000`. Nearly ten minutes of work thrown away on a string comparison.

## Result

```
boot 1   injected=true  stage4_pass=0  fresh=1  last_stage=stage1  stalls=1  reboots=3
boot 2   injected=true  stage4_pass=0  fresh=0  last_stage=none    stalls=1  reboots=1
pass=0   rule=stage4_pass==1 AND firstboot_fresh==1
```

0 of 2. Interactive gates on this image reached 11/12, so this is a real
difference between the two environments, not a known-flaky boot.

## What the guest logs say

Boot 1 reached stage1 and rebooted to activate testsigning, which is the normal
path. It never reached stage2.

```
[boot-identity] path=C:\BridgeVM\stage1.boot value=0xad
[stage1] rebooting to activate testsigning
```

After that reboot the diagnostics runner entered at 02:08:10 UTC and wrote
nothing further. The watchdog fired at 02:46. Thirty-eight minutes of silence
after re-entry.

A caution about reading these directories: most files under `guest-logs/` are
not from this run. `bvgpu-diagnostics-latest.log` ends 07/18/2026 -- it came
with the base image. Only `bvgpu-runner-entry.log` (08/05) and the firstboot
log are this boot's. Comparing stage markers between a queued run and an
interactive one is only meaningful for those.

## What is not known

Why the runner stops after re-entry, and why it differs from an interactive
run, is unresolved. Candidates, none tested:

- `caffeinate -dimsu` wraps the queued run and not the interactive one
- the queue runs under launchd with a different session and no controlling tty
- concurrency: this gate ran while the A19 snapshot gate was hashing 64 GiB

The third is the cheapest to rule out and the easiest to get wrong by
assumption, so it should be measured rather than argued about.

## What did work

The prepared cache did its job: one injector pass, `ready` written only after
`injector_boot_observed=true`, and both boots cloned from it. That is the
2x saving it was built for, and it is now proven end to end.
