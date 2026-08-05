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

## Second run: the cause was in the harness, not the guest

Two more defects turned up, and fixing them changed the result.

**The prepared cache never hit.** Its key was built from `stat()` -- device,
inode, mtime. A git worktree gives every file a new inode and timestamp, so the
key changed on every commit. Every queued gate paid ten minutes for an injector
pass to populate a directory nothing would ever read. This is the same mistake
as the injector freshness check fixed earlier the same day; making it twice
says the first lesson was filed under that one check rather than under
timestamps. Now keyed on content, and proven to produce the same value from the
repo and from a worktree at a different path (`919f6aba...`).

**Kill mode did not kill.** Boot 1 of the second attempt logged
`boot-progress watchdog ending run (kill mode)` and then ran another eight
minutes, reaching 48 against a 40-minute deadline. Kill mode called
`hv_vcpus_exit` and nothing else, and the run loop treats a canceled exit with
no flag set as a surplus automation wake -- correct for every other waker,
wrong for this one. It now carries its own run-scoped flag.

With both fixed, on `23ec5f3`:

```
boot 1   injected=true  stage4_pass=1  fresh=1  last_stage=stage4  stalls=0  reboots=3
boot 2   injected=true  stage4_pass=0  fresh=1  last_stage=stage1  stalls=1  reboots=2
pass=1
```

Boot 1 is the first A1-passing boot ever produced from the queue: stage4
reached, fresh firstboot, no stalls. Boot 2 stalled at stage1 and kill mode
ended it in **8 minutes** rather than sitting out the full 40 (boot 1, which
passed, took 18).

So the 0/2 above was not the guest behaving differently under launchd. It was a
cache that never hit and a watchdog that never stopped anything.

## What is not known

1 of 2 is not 9 of 10. The stage1 stall in boot 2 is the same shape as the one
this criterion has been stuck on: firstboot reboots to activate testsigning and
does not come back. Whether the queued rate differs from the interactive 11/12
needs more boots than a pilot runs -- that is what t4-soak and t5-campaign are
for, and they are now unblocked.

## What did work

The prepared cache did its job: one injector pass, `ready` written only after
`injector_boot_observed=true`, and both boots cloned from it. That is the
2x saving it was built for, and it is now proven end to end.
