# T1: bare-metal vtimer/cancellation microprobe (2026-08-04)

## Why

The A1 boot stall leaves a Windows guest parked in `WFI` with `CNTV_CVAL` in
the past and no further wake ever arriving. Every hypothesis about it has so
far been tested by running a full Windows boot, which costs roughly 20 minutes
per sample and gives one bit of information. That is why the A1 investigation
has moved slowly and why several plausible causal claims were later retracted.

This probe reproduces the host-side condition against a 39-instruction EL1
guest, so a hypothesis can be tested in seconds.

## What the probe does

`crates/bridgevm-hvf/examples/hvf_vtimer_cancel_probe.rs` builds a minimal VM
with Apple's in-kernel GIC, maps one page of guest code, and runs an idle loop
that models how an operating system parks:

```
arm:   clear fired flag; CNTV_CVAL = now + delta; CNTV_CTL = ENABLE
wait:  WFI; if fired flag clear -> back to WFI   (do NOT re-arm)
       else -> arm
```

A second thread calls `hv_vcpus_exit` continuously, racing cancellations
against timer fires. The guest source is checked in beside the probe
(`guest_main.s`, `guest_handler.s`) and the assembled words are pinned by unit
tests, so a corrupted blob fails rather than silently running something else.

## The first version proved nothing, and why

The initial guest re-armed the timer on **every** `WFI` wake. That silently
repaired every swallowed fire, so the run passed even with recovery disabled.
A probe whose negative control passes is measuring nothing.

The fix — parking again on a spurious wake without re-arming — is what makes a
lost fire fatal in the probe exactly as it is in a real guest, where nothing
re-arms on the guest's behalf. This is recorded because the first version
looked like a working probe and produced confident, meaningless output.

A second false signal came from the swallowed-fire detector. Checking only
"masked and deadline past" matched ~100,000 times per run, because it also
matches the benign window between servicing an interrupt and re-arming. Gating
on `PC == the WFI address` separates "parked" from "in flight".

A third: the quiesce experiment first reported "the fire was lost" after 20µs.
That was a stale `EXIT_CANCELED` from before the pause, not an answer. The
experiment now keeps running until either the wake arrives or a full 500ms
quiet window elapses.

## Result

With cancellation at maximum pressure, a guest parked at `WFI`:

```
PC=0x4000008c masked=true CNTV_CTL=0x5 overdue_ticks=13,829,640
waited 500ms in a fully quiet window, woke=false
```

`CNTV_CTL=0x5` is `ENABLE=1, ISTATUS=1`: the timer condition **is** met and the
interrupt is pending, but the vtimer is masked, so it is never delivered. With
every cancellation stopped for half a second, the wake still never arrives.
This is a permanently lost wake, reproduced in seconds and confirmed on three
consecutive runs.

The recovery in `probe_runtime/vtimer_recovery.rs` — unmask, and if the
deadline has already passed rewrite `CVAL` to now — rescues this state every
time.

### Gate run

`scripts/run-hvf-vtimer-cancel-gate.sh --out <dir>`:

| field | value |
| --- | --- |
| `iterations` | 10000 |
| `timer_wakes` | 10000 |
| `canceled_exits` | 3,586,849 |
| `masked_past_deadline` | 4,942 |
| `swallowed_unrecovered` | **0** |
| `vtimer_exits` | 0 |
| `outcome` | completed |
| `elapsed_ms` | 12,321 |

Negative control, `--no-recover`, 2000 iterations: `swallowed_unrecovered 699`,
`pass: false`, exit 1. The gate fails when the fix is removed, which is what
makes the pass meaningful.

Wall time is ~12s against the plan's T1 SLO of 2 minutes.

## What this does and does not establish

Established:

- Apple HVF can leave a vCPU parked in `WFI` with a pending, masked virtual
  timer that is never delivered, and cancellation pressure produces it.
- That state is permanent without host intervention, not transient.
- The shipped recovery clears it, across 10,000 iterations and ~3.6M cancels.

Not established:

- **That this is the cause of the A1 boot stall.** The probe shows the
  mechanism exists and is reachable; it does not show it is what a failing
  Windows boot hit. A1 remains open at 11/12.
- Nothing about Windows-specific behaviour: SMP, the GIC redistributor state a
  real guest programs, or `KeIpiGenericCall`. The probe has one vCPU and 39
  instructions.

The correct use of this gate is as a fast precondition: a candidate vtimer fix
that cannot pass here does not deserve a boot campaign. Passing here does not
substitute for the two independent 10-boot campaigns A1 release promotion
requires.

## Reproducing

```bash
scripts/run-hvf-vtimer-cancel-gate.sh --out "$HOME/BridgeVM/runs/vtimer-cancel-$(date +%Y%m%d-%H%M%S)"

# negative control: must fail
scripts/run-hvf-vtimer-cancel-gate.sh --skip-build --iterations 2000 --no-recover

# is a swallowed fire lost or merely delayed?
scripts/run-hvf-vtimer-cancel-gate.sh --skip-build --iterations 1500 \
  --no-recover --quiesce-probe --cancel-interval-us 0
```
