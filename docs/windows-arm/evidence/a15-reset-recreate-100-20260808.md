# A15 PROVEN: 100 consecutive SYSTEM_RESET cycles on the userspace GIC

## Run

    BRIDGEVM_USERSPACE_GIC=1 CYCLES=100 scripts/verify-reset-recreate.sh
    OUT=/Users/insighton/BridgeVM/runs/reset-recreate-usgic3-151732

Canonical installed image (`canonical-attach-resident-20260731`), cp -c
clone, 6144 MiB / 4 vCPUs, probe in `--exit-on-reset` product shape: every
guest SYSTEM_RESET exits the helper (code 42) and the supervisor launches a
fresh process.

## Result

    PASS: 100 reset cycles, each with a fresh helper PID, increasing
    generation, fresh agent READY, 4 CPUs
    - agent READY: 100/100 cycles (fresh boot marker each generation)
    - online_cpus=4: 100/100
    - helper exited 42 (guest reset): 100/100
    - reset.receipt: generation 100, disk+vars flushed

Wall clock ~4.7 h, zero A1-class parks in 100 consecutive boots. Prior
best on the in-kernel GIC: 8 cycles/13 generations before a UEFI park.

## Two harness fixes this run forced

1. **PID-uniqueness compares against the predecessor only**: macOS recycles
   PIDs; a 100-cycle run wraps and the all-history comparison failed
   spuriously at cycle 17. The criterion's "new helper process" means the
   previous helper is gone and the new PID differs from it.
2. bash-3.2 empty-array guard for the predecessor lookup.

## Reading

The reset/recreate machinery was never the problem — the per-boot A1
mortality was. With the userspace GICv3 the failure term is gone:
100 consecutive full Windows boots, each through firmware + winload +
ntoskrnl SMP bringup + agent service start, without one park.
