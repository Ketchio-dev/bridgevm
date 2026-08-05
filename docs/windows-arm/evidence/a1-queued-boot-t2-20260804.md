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

## t4-soak: 2 of 5, and a single failure shape

Five boots on `84a1463`, the first measurement of the guest rather than the
harness:

```
boot 1  stage4_pass=1  fresh=1  last_stage=stage4  stalls=0  reboots=3   19 min
boot 2  stage4_pass=0  fresh=0  last_stage=none    stalls=1  reboots=1    4 min
boot 3  stage4_pass=0  fresh=0  last_stage=none    stalls=1  reboots=1    4 min
boot 4  stage4_pass=1  fresh=1  last_stage=stage4  stalls=0  reboots=3   17 min
boot 5  stage4_pass=0  fresh=1  last_stage=stage1  stalls=1  reboots=1    6 min
pass=2
```

The split is clean. A boot either reaches stage4 through three reboots in
17-19 minutes, or it stalls after its **first** reboot and is killed in 4-6.
Nothing lands in between.

All three failures carry the identical fingerprint:

```
boot-progress watchdog stalled_for_ms=120000 exits_in_window=0
    total_exits=~170000 reboots=1 suspect=guest-not-running
```

Two full minutes with zero vCPU exits, immediately after the first reboot.
Whatever happens, happens in the reboot the firstboot script triggers to
activate testsigning -- the guest goes away and does not come back.

Kill mode is what makes this cheap to observe: these three cost 14 minutes
between them instead of 120.

## What is not known

2 of 5 against a 9-of-10 gate, and against 11/12 measured interactively on the
same image. Either the two environments differ in a way not yet found, or the
interactive figure was luckier than it looked; 12 boots is a small sample for a
rate this far from 1.

What is now pinned down is *where* it goes wrong: not during install, not at
stage4, but in the first reboot. `exits_in_window=0` says the vCPU is not
executing, which is stronger than "the guest is quiet". The next step is to
capture GIC state across that reboot rather than guessing between the
candidates -- the two-snapshot method built for exactly this question.

## What did work

The prepared cache did its job: one injector pass, `ready` written only after
`injector_boot_observed=true`, and both boots cloned from it. That is the
2x saving it was built for, and it is now proven end to end.
