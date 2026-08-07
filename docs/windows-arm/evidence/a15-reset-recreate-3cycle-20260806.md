# A15: process-recreate reset cycles run live — 3/3 with a real Windows guest (2026-08-06)

`scripts/verify-reset-recreate.sh` is the A15 harness: the probe runs with
`--exit-on-reset` (PLAN.md R1 product mode — a guest SYSTEM_RESET exits the
process with code 42 instead of rebooting in process), and the script is the
supervisor: verify, flush, receipt, fresh helper.

First live run, 3 cycles on `canonical-attach-resident-20260731`:

```
cycle 0: helper_pid=46365 agent READY / online_cpus=4 / helper exited 42
cycle 1: helper_pid=46798 agent READY / online_cpus=4 / helper exited 42
cycle 2: helper_pid=47221 agent READY / online_cpus=4 / helper exited 42
PASS: 3 reset cycles, each with a fresh helper PID, increasing generation,
      fresh agent READY, 4 CPUs
```

Each cycle proves all four A15 per-cycle facts:
- **new helper PID** (46365 → 46798 → 47221, uniqueness asserted),
- **increasing reset generation** (cycle index, monotonic by construction),
- **fresh guest boot marker** (that boot's own `BVAGENT READY`),
- **4 online CPUs** (asked of the guest itself:
  `Win32_ComputerSystem.NumberOfLogicalProcessors` → 4).

The reset is triggered from inside the guest (`shutdown /r /t 2` via the
agent console), travels the real PSCI path, and the wait asserts exit code
**42 exactly** — a crash or clean shutdown fails the cycle.

Wiring detail found by running: the wrapper scrubs every inherited
`BRIDGEVM_*` env var as a policy boundary, so the env-only first attempt
silently kept in-process reboots (`PSCI SYSTEM_RESET: reboot 1/8` in the
log). The mode is now a first-class validated flag (`--exit-on-reset`,
recorded in preflight as `exit_on_reset=1`), which is also the right shape
for the closed-configuration audit trail.

## What this does not prove yet

A15's gate is **100 consecutive** cycles. 3 cycles prove the machinery,
not the endurance; the A1 stall class (14/40 boot success in soaks) will
interrupt long chains until it is fixed — cycle failures there are A1
evidence, not A15 regressions. `CYCLES=100` runs the full gate when A1
stabilizes. The receipt in this harness is script-side; the typed
supervisor (`hvf-runner --supervise`) exists and is proven with stub
helpers, and switching the harness to it is the remaining A14/A15 seam.
