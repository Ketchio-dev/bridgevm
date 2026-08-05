# The intermittent boot stall that blocks A1

Status: **substantially reduced, not closed.** A1 remains open.

The stall is a lost virtual-timer wake around host-initiated vCPU cancellation.
Recovering the timer on every canceled exit (`dd273c5`) moved the diagnostic
gate from 17/20 to 11/12, but one boot still stalled with an expired, *unmasked*
timer, so the mechanism is not yet a complete root-cause proof. The reasoning
below is kept in the order it was discovered, including the hypotheses that were
wrong.

## Rate

Twenty cold boots across two ten-boot gates on the same build and image:

| gate | pass |
| --- | --- |
| `a1-gate10` | 9/10 |
| `a1-gate10b` | 8/10 |
| **combined** | **17/20 = 85%** |

A1 requires 90%, so the stall alone decides the criterion. At 15% it is far too
frequent to dismiss.

After the vtimer recovery work the same gate shape reached **11/12**; see the
three fix attempts near the end of this document.

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

## A dead boot entry left by the injector pass — real, but NOT the cause

Reading the failing framebuffer rather than guessing at it gave the answer
directly (`a1-boot-stall-bdsdxe-not-found-20260803.png`):

```
BdsDxe: failed to load Boot0003 "Windows Boot Manager" from
HD(1,GPT,A0A0C780-D438-47C2-8C96-1B56363C72DD,0x800,0x2FF000)
  /EFI/Boot/bootaa64.efi: Not Found
```

That GUID is the **injector's** partition, pinned deliberately by
`build-hvf-windows-driver-injector.sh:19` so the known-good vars can address it
as `Boot0003`.

The gate passes the *same* `vars.fd` to both passes (`p1-boot-gate.sh:105,112,123`).
Parsing the vars image confirms the last valid `BootOrder` record is
`[0x0, 0x3, 0x0]` — so on pass 2, with the injector disk gone, firmware tries
`Boot0003`, cannot find `bootaa64.efi`, and has to fall back to `Boot0000`.

It was tempting to stop here: this explains every property observed earlier —
it always follows a reset (each reset re-runs BDS), it lands at different
firstboot stages (any reset can hit it), and it leaves the TianoCore "Start boot
option" frame. **That reasoning was wrong**; see the measurement below.

Two of the four failures also share a register fingerprint —
`x1=0xc x2=0x2 x3=0xe01`, `lr` low bits `0x2b0`, and an identical
`x28=0x478d4510` — differing only by ASLR, so they are the same code path.

The error is not itself the bug: the passing run `r5` logs it **24 times** and
boots anyway. The bug is that the fallback is occasionally fatal.

## The dead entry was removed, and the stall did not go away

`scripts/drop-injector-boot-entry.py` removes `Boot0003` from `BootOrder`
between the two passes. It works exactly as intended — on a verification boot,
`BootOrder ['0x0','0x3','0x0'] -> ['0x0','0x0','0x0']` and `bds_errors` drops
from 24 to **0**.

The failure rate did not move:

| gate | pass | `bds_errors` |
| --- | --- | --- |
| `a1-gate10` | 9/10 | present |
| `a1-gate10b` | 8/10 | present |
| **`a1-gate-fixed`** | **8/10** | **0** |

Before: 17/20 = 85%. After: 8/10 = 80%. **No improvement.**

The failing boot in the fixed gate, `boot-10`, confirms it directly: its log
contains zero `Not Found` errors, and it still stalls with
`exits_in_window=0 suspect=guest-not-running` after reboot 3, still ending on
the "Start boot option" frame — this time with no error text above it at all.

**Conclusion: the dead boot entry was a real defect and a genuine cleanup, but
it is not the cause of the stall.** The stall is inside BDS's normal handoff,
independent of which boot option is being tried.

The fix is kept — it removes 24 spurious firmware errors per boot and is
correct on its own terms — but it must not be described as fixing A1.

## The stall is inside the NT kernel: `KeIpiGenericCall` waiting for IPI acks

Symbolised offline with the guest's own binaries. `ntoskrnl.exe` was extracted
from the canonical image; its RSDS record names `ntkrnlmp.pdb`
`{00D46CC0-E2A1-3E48-6AC6-C224799CFC23}` age 6, fetched from the Microsoft
symbol server and matched.

Every kernel-mode stall sample has the same PC low bits. Candidate RVAs whose
low 20 bits are `0x761dc` were read back from the PE and exactly one carries
the observed instruction pair `word_before=0xb9008268 word_at=0xb9427f68`:

```
rva=0x2761dc word_at=0xb9427f68 word_before=0xb9008268 <== MATCH
```

`llvm-symbolizer` places `0x2761dc` in **`KeIpiGenericCall`** (public RVA
`0x276140`, so the stall PC is `+0x9c` and the sampled LR `+0x170` is inside
the same function). The loop decodes to:

```
+0x9c: ldr  w8, [x27, #0x27c]   ; read target-CPU acknowledgement mask
+0xa0: ands wzr, w9, w8
+0xa4: b.eq +0xd0               ; done when every target acked
+0xac: yield
+0xb0: ldr  w8, [x19]           ; inner wait, loops back
```

That is the textbook "wait for all other CPUs to acknowledge the IPI" spin.
It also explains the watchdog's `exits_in_window=0`: a YIELD loop performs no
MMIO and takes no traps, so the host sees a guest that is "not running" while
CPU0 is in fact spinning at full speed waiting for CPUs that never answer.

Three independent stalls confirm byte-identical registers modulo KASLR:
`pc` low `0x761dc`, `lr` low `0x762b0`, stuck word `0xb9427f68`, and
`x27 - pc == 0xb91e24` in every sample (`a1-gate10b/boot-9`,
`a1-gate-fixed/boot-10`, `a1-par-test/boot-5`).

Two corrections this forces:

- **The "Start boot option" frame was misread.** ramfb only shows firmware
  output; once Windows takes over the display via the GPU miniport the ramfb
  frame goes stale. The kernel-mode stalls end on that frame *because nothing
  updates ramfb after handoff*, not because the guest is still in BDS. The
  two pre-fix failures with UEFI-space PCs (`psci 8`) remain genuine firmware
  hangs — there are two distinct modes.
- **The psci-count retraction needs partial un-retracting.** For the
  kernel-mode stalls the count is a clean discriminator after all: every one
  reports `psci 26` where every passing boot reports `psci 27`. The earlier
  "refutation" compared a kernel-mode failure against the UEFI-mode pattern.

Why an IPI would be lost is the open question, and it is a host-side one:
either SGI delivery after the Nth `hv_gic_reset` drops, or one secondary vCPU
is not actually running when the kernel targets it. Both live in our reset /
secondary-respawn path, not in Windows.

## SMP trace answers it: `CPU_ON` is never issued at all

Reproduced with `BRIDGEVM_SMP_TRACE=1` (`a1-smp/boot-1`, stalled after
reboot 3). The full timeline per reboot:

Successful reboot (reboot 2 of the same run):

```
PSCI SYSTEM_RESET: reboot 2/8
vCPU1 blocking while Off        (all three secondaries parked)
...cpu0 progresses to ~70000 exits...
vCPU1 Off -> OnPending          (kernel issues CPU_ON)
vCPU1 created HVF vCPU 1 / OnPending -> On
vCPU2, vCPU3 likewise
```

Failed reboot (reboot 3):

```
PSCI SYSTEM_RESET: reboot 3/8
vCPU2/3/1 blocking while Off    (all three parked normally)
progress cpu0_exits=10000..60000 secondary_exits=0
probe: boot-progress watchdog stalled_for_ms=120000 exits_in_window=0
vCPU2/1/3 woke with state Off   (threads alive, answering shutdown)
```

Three facts follow directly:

1. **The secondaries are healthy.** All three park in `Off`, their threads
   respond at shutdown, and the respawn machinery was never even asked to run.
   The "secondary not running when targeted" hypothesis is dead.
2. **`CPU_ON` is never issued.** Not lost, not answered `ALREADY_ON` — the
   guest kernel never makes the call (`Off -> OnPending` count: zero).
3. **cpu0 stops exiting at ~60000 exits**, right where the passing reboot is
   ~10000 exits away from issuing its first `CPU_ON`, and its final PC is the
   `KeIpiGenericCall` ack spin.

So the guest kernel, still effectively single-CPU, enters a wait for other
CPUs' IPI acknowledgements before ever starting those CPUs. The kernel only
does that if something it read tells it other processors are already up — or
if its own bring-up path wedged earlier than the `CPU_ON` call. Either way the
input it acts on comes from us: GIC redistributor state after the Nth
`hv_gic_reset`, or our PSCI `AFFINITY_INFO` answers.

Caveat for reproduction cost: with the trace enabled all 6 boots of the gate
failed (previous rate ~15%), so the tracing itself perturbs timing — an
observer effect worth remembering, and also why this sample was cheap to get.

## The two stall modes share one symptom: a lost virtual-timer wake

The GICR-read hypothesis above was not what the state showed. Reading the timer
state at the moment of the stall unified both failure shapes:

| stall site | `CNTV_CTL` | `ISTATUS` | CVAL |
| --- | --- | --- | --- |
| kernel idle | `0x1` | 0 | already in the past |
| UEFI WFI | `0x5` | 1 | expired, no wake |

In both cases the guest is waiting for a virtual timer that will never fire
again. `vtimer_exits=0` in passing *and* failing boots, because Apple's
in-kernel GIC delivers the timer directly without a guest exit — so exit
counters could never have seen this.

Adding `hv_vcpu_get_vtimer_mask` telemetry showed stalled boots ending with
`masked=true`: the host had left the vtimer masked, so the expiry was swallowed.
At that stage the archived correlation was perfect — 10/10 sampled stalled boots
had `surplus-canceled=1`, and 11/11 sampled passing boots had
`surplus-canceled=0`.

## Three fix attempts, and what each one disproved

| commit | change | gate | what it proved |
| --- | --- | --- | --- |
| `8c24cca` | unmask on surplus-canceled exits | `a1-fix` 10/12 | Fixed its narrow condition: failures changed to `masked=false`. Did not re-arm an already-expired timer. |
| `5aac956` | also rewrite an expired CVAL to guest-now | `a1-fix2` 10/12 | Disproved the assumption that only *surplus* cancels swallow a timer; a later claimed cancel does it too. |
| `dd273c5` | recover on **every** `EXIT_CANCELED` | `a1-fix3` **11/12** | Materially improved the gate, but did not close it. |

The first two commits also exceeded four structural budgets; the shared logic
now lives in
`crates/bridgevm-hvf/examples/hvf_gic_boot_probe/probe_runtime/vtimer_recovery.rs`
and every original ceiling was restored.

## Why A1 is still open

`a1-fix3/boot-7` failed after one reboot with `CNTV_CTL=0x1`,
`masked=false`, `surplus-canceled=1`, and the same `KeIpiGenericCall+0x9c` final
PC. The timer was neither masked nor pending: the recovery ran and the guest
still did not wake.

So the canceled-vtimer race is a **strongly measured cause, not a completed
root-cause proof**. Apple's auto-mask and edge-latch behaviour around
`hv_vcpus_exit` is not documented well enough to treat the hypothesis as
finished, and 11/12 is below the 9/10-with-two-independent-campaigns bar this
project requires before promoting A1.

## Next step

Build a bare-metal vtimer/cancellation microprobe that races
`hv_vcpus_exit` against timer expiry for 10,000 iterations without booting
Windows. If same-CVAL re-evaluation cannot recover every iteration, the
correctness boundary moves to removing the competing wake sources or recreating
the VM process on reset, rather than repairing the timer after the fact.

### Outcome of that step

Built and run: see
[`t1-vtimer-cancel-microprobe-20260804.md`](t1-vtimer-cancel-microprobe-20260804.md).

It establishes that the mechanism is real and reachable. A single vCPU parked
in `WFI` can be left with `CNTV_CTL=0x5` -- the timer condition met and the
interrupt pending -- while the vtimer stays masked, and the wake is then never
delivered even across a 500ms window with all cancellation stopped. The shipped
recovery clears that state across 10,000 iterations and ~3.6M cancels.

It does **not** close A1. The probe shows the mechanism exists; it does not
show it is what `a1-fix3/boot-7` hit, and that boot recorded `masked=false`
with the recovery already run. A1 stays open at 11/12, and the residual failure
still needs a Windows-side explanation -- SMP, the real guest's redistributor
programming, or `KeIpiGenericCall` itself.


## 2026-08-05: the failing boot never leaves UEFI

Five-boot soaks from the queue put a sharper edge on this. Comparing the
final `REGS` line of a passing boot against a failing one:

```
pass:  pc=0xfffff802d7a999fc  lr=0xfffff802d7a833b4   (Windows kernel range)
fail:  pc=0x1bf33ba04         lr=0x478e2a14           (UEFI range)
```

The failing boot is not a Windows kernel that stalls. It is a guest that never
reaches the kernel at all: after the reboot firstboot triggers to activate
testsigning, control stays in firmware. `0x1bf33ba04` is the same UEFI PC
recorded in the original fingerprint for this criterion, so the two are the
same failure, and it is a firmware-handoff problem rather than a
`KeIpiGenericCall` problem.

Timings agree. A passing boot takes 17-19 minutes through three reboots. A
failing one is confirmed stalled 4-6 minutes in, with `exits_in_window=0` and
`reboots=1`.

### A retraction

The first version of the stall-time GIC snapshot printed:

```
GIC SNAPSHOT: PC 0x0 -> 0x0 psci_state=0 vtimer_masked=false
GIC SNAPSHOT: CNTV_CTL=0x0 ... verdict=parked
```

None of that was real. HVF only permits register reads from the thread that
owns the vCPU, the snapshot ran on the watchdog thread, and `capture()`
discards every status code -- so it reported zeros, and "PC did not move" plus
"verdict=parked" are precisely the conclusions the snapshot exists to support.
Three lines later the same log shows the true PC as `0x1bf33ba04`.

The code now checks a read's status first and says it cannot report rather than
reporting zeros. Recording this because the fabricated version was live, and
anyone reading that log would have drawn a confident and wrong conclusion.

### The duplicate BootOrder entries are load-bearing

`drop-injector-boot-entry.py` pads BootOrder after removing the injector's
Boot0003, turning `[0x0, 0x3, 0x0]` into `[0x0, 0x0, 0x0]`. That looked wrong:
the padding is justified by a comment claiming firmware treats a repeated entry
as already tried, which was never tested, and the source image lists Boot0000
twice regardless.

Trimming it to a single `[0x0]` was measured against the same soak:

| BootOrder | result | failure shape |
|---|---|---|
| `[0x0, 0x0, 0x0]` | 2 of 5 pass | stall at `reboots=1` |
| `[0x0]` | 0 of 2 pass | no reboot at all, `reboots=0` |

With one entry the guest stops rebooting entirely and sits at a UEFI PC until
the 40-minute deadline. So the duplicates are doing something real, and the
change was reverted.

What that something is remains unknown. A plausible reading is that firmware
needs more than one attempt at Boot0000 -- the first failing and a retry
succeeding -- which would make the "already tried" comment exactly backwards.
That is a hypothesis, not a finding.
