# A15: 100 consecutive SYSTEM_RESET cycles on the in-kernel GIC

The 2026-08-08 A15 run used the userspace GICv3 (`BRIDGEVM_USERSPACE_GIC=1`),
which the app never enabled and which was deleted in #709. This run repeats
the criterion on the in-kernel GIC, the only interrupt controller the app has.

## Run

    TARGET=disk.raw VARS=vars.fd OUT=<new dir> CYCLES=100 \
      BRIDGEVM_PREBUILT_PROBE=<packaged release probe> \
      scripts/verify-reset-recreate.sh

- Source: main `d7e76a5e`; the release probe from an ad-hoc package of the
  same runtime sources (sha256 `6d67ccc0eac068be…`).
- Guest: the installed Windows 11 ARM64 image used by the A19 lanes, as a
  private APFS clone with its own vars copy, deleted afterwards. 6144 MiB,
  4 vCPUs, `--exit-on-reset` (the product shape: a guest reset ends the
  helper with exit 42 and the supervisor starts a fresh process).
- Host: Mac Studio, Apple M4 Max, macOS 27.0.1, 2026-11-23 UTC. No other VM
  was running.

## Result

| Run | Cycles | Helper processes | Distinct guest boot times | 4 CPUs | Wall clock | Agent service start after launch |
|---|---|---|---|---|---|---|
| pre-commit draft of the same script, 04:26–04:54Z | 100/100 | 100 | 100, strictly increasing | 100/100 | 28 min | p50 8 s, p95 11 s, max 19 s |
| `scripts/verify-reset-recreate.sh` as committed, 05:17–05:45Z | 100/100 | 100 | 100, strictly increasing | 100/100 | 28 min | p50 8 s, p95 9 s, max 19 s |

- Every reset ended its helper with exit 42; the next cycle's helper had a new
  PID and its predecessor was gone. No intermediate reset happened before an
  agent start, and the final cycle ended with a clean guest shutdown (exit 0).
- Every helper's log ends with the probe's GIC snapshot taken at stop, and all
  200 say `stall=false`. No boot parked in firmware or in the kernel (the A1
  shapes).
- The first cycle of each run is the slowest (19 s): it starts from a cold
  page cache.

## Reading

The A15 statement holds on the shipped interrupt controller: 100 consecutive
guest resets, each in a fresh helper process with a new PID, an increasing
generation, a fresh guest boot and 4 online CPUs. Rank: local physical-Mac
run, not a queue receipt.

The 2026-08-08 record's "prior in-kernel-GIC best: 8 cycles before a UEFI
park" predates the A1 fix of 2026-08-10 (in-kernel A1 gate 10/10) and the
removal of the probe's vtimer recovery code; neither park shape appeared here.
