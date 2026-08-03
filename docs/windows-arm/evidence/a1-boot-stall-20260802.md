# The intermittent boot stall that blocks A1

Status: characterised, not fixed. This is the sole remaining obstacle to A1.

## Rate

Twenty cold boots across two ten-boot gates on the same build and image:

| gate | pass |
| --- | --- |
| `a1-gate10` | 9/10 |
| `a1-gate10b` | 8/10 |
| **combined** | **17/20 = 85%** |

A1 requires 90%, so the stall alone decides the criterion. At 15% it is far too
frequent to dismiss.

## Signature

All three failures look identical in the probe's own terms:

```
probe: boot-progress watchdog stalled_for_ms=120000 exits_in_window=0
       total_exits=... reboots=N suspect=guest-not-running
```

`exits_in_window=0` is the important field: over a two-minute window the guest
produced **no vCPU exits at all**. It is not slow, not spinning, not waiting on
a device — it is not executing.

## What varies, and what does not

Everything about *where* it happens varies:

| run | last firstboot stage | reboot when it died | final PC |
| --- | --- | --- | --- |
| `a1-gate10/boot-2` | stage3 | 3 | `0x478e2fbc` (UEFI) |
| `a1-gate10b/boot-1` | stage1 | 2 | `0x1bf33b6ac` (UEFI) |
| `a1-gate10b/boot-9` | none | 1 | `0xfffff8001a4761dc` (kernel) |

Three different stages, three different reboot counts, and final PCs in two
different address spaces. So this is not a bug in any one firstboot stage.

The one constant: **every failure happened immediately after a reboot.** The
guest went down for a `PSCI SYSTEM_RESET` and did not come back up.

## The frame that separates pass from fail

Two of the three failures left an identical final framebuffer,
`checksum64=0xaf552b4d7621db7e` — the TianoCore splash with **"Start boot
option"** on screen (`a1-boot-stall-start-boot-option-20260802.png`). That is
the moment UEFI hands control to the boot loader.

That frame is a clean discriminator:

| run | frames matching `af552b4d` | pass |
| --- | --- | --- |
| `a1-gate10b/boot-5` | 0 | yes |
| `a1-gate10b/boot-3` | 0 | yes |
| `a1-gate10/boot-2` | 2 | no |
| `a1-gate10b/boot-1` | 2 | no |

No passing boot ever ends on it. **The stall is in firmware handoff, before
Windows starts**, which is consistent with the two UEFI-space PCs. The third
failure died with a kernel PC and no such frame, so it is either a second mode
or a later manifestation of the same one — not yet established.

## Hypothesis tested and rejected

In the first gate the single failure was the only boot reporting `psci 8` where
passing boots reported 27, and its BAR2 `base_changes` counter stopped at 19
against 23 for every pass. That looked decisive.

The second gate refuted it: `a1-gate10b/boot-9` failed with `psci 26` and
`base_changes=7`. **PSCI call count is not the discriminator.** Recorded so the
next investigator does not spend a gate re-deriving it.

## Not the cause

- **Not stale UEFI variables.** The gate copies `BASE_VARS` fresh for every
  iteration (`p1-boot-gate.sh:105`), so no boot inherits another's NVRAM.
- **Not the injector pass.** All three failing runs reported
  `injector_boot_observed=true` with exactly one vars write-back, identical to
  passing runs.
- **Not a graphics defect.** The guest is not running, so nothing has been
  submitted to the GPU. The venus fence-poll watchdog correctly reports
  `contexts=0 outstanding_fences=0 suspect=idle-no-outstanding-fence`.

## Reproduced outside the gate

`/tmp/reboot-stress.sh` reproduces it without any firstboot-stage analysis:
injector pass, then boot installed Windows, repeated. Eight iterations gave
**7 clean, 1 stall** — the same rate as the gates, with the same signature:

```
PSCI SYSTEM_RESET: reboot 4/8
probe: boot-progress watchdog stalled_for_ms=120000 exits_in_window=0
       total_exits=523484 reboots=4 suspect=guest-not-running
```

This gives a cheaper reproduction than a full ten-boot gate.

### What that run rules out

An earlier attempt at a stress harness ran twelve iterations of a plain boot and
got 12/12 clean — but its logs show `reboots=0`. The image boots straight to the
desktop, so the reset path was never exercised at all. A second attempt tried to
force reboots through the guest agent and also got `reboots=0`, because
`wall-c8-clean-12041.raw` has no agent installed (`agent_confirmed=false`).
Neither result is evidence of anything; only the third harness, which reaches
`reboots=3` normally, is a valid test.

That is itself a useful negative: **the stall requires guest reboots to
reproduce**, consistent with every observed failure following a reset.

The failing iteration took **four** reboots where all seven passing ones took
three, meaning the guest rebooted once more than the firstboot script asks for.
Whether that extra reset is a cause or a symptom is not yet established.

Comparing the log immediately after each reset shows the failing and passing
runs are byte-identical through `hv_gic_reset`, the PMUVer fixup, the
redistributor base and the first ramfb checkpoint. The divergence is after that
point, not in the reset sequence itself.

## Next step

The evidence points at the UEFI reset path rather than at Windows. Whether it
shares a root cause with the BAR2 stale-cache defect fixed in `e6758f4` is
**unproven** and should not be assumed.
