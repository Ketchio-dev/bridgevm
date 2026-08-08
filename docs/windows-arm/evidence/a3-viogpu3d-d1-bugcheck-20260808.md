# A3 blocker relocated: viogpu3d.sys D1 bugcheck at PPSSPP D3D11 startup (userspace GIC)

## What changed

On the userspace GICv3 the A3 gate no longer parks A1-class — the guest now
gets far enough to CRASH deterministically. Two consecutive gate runs
(a3-gate-142322, a3-usgic-diag2), same signature:

    DRIVER_IRQL_NOT_LESS_OR_EQUAL (0xD1)
    P1 = P4 = <viogpu3d base> + 0x1400c   (referenced == faulting PC)
    P2 = 2 (DISPATCH_LEVEL)
    P3 = 8 (EXECUTE fault)

Run 1: 0xfffff80197ab400c (base 0xfffff80197aa0000)
Run 2: 0xfffff8021c25400c (base 0xfffff8021c240000)
Identical RVA 0x1400c both times.

## Minidump analysis (guest minidump pulled through the share)

`080826-8828-01.dmp` (PAGEDU64 triage dump), module list places the fault in
**viogpu3d.sys** (driver-list entry: base+0x14000 region, size 0x2a000; the
name resolves to viogpu3d.sys in the string pool).

Section table of the shipped `viogpu3d.sys` (download-120.45-backing-only,
venus WIP + BridgeVM patch stack):

    PAGE  va=0x13000 vsz=0x13b41   <-- RVA 0x1400c lands here

Disassembly at the site:

    0x13fe8: f9412a10  ldr  x16, [x16, #0x250]
    0x13fec: d63f0200  blr  x16          ; indirect call through +0x250 slot
    0x13ff0: d63f01e0  blr  x15
    ...
    0x14004: 17ffffdd  b    -0x8c        ; last insn of the function
    0x14008: ff676980                    ; literal pool
    0x1400c: ffffffff                    ; inter-function padding  <== PC

The crash is an indirect call whose target resolves to the PADDING between
two functions in the pageable PAGE section, executed at DISPATCH_LEVEL.

**Correction (same day):** "not a VMM defect" was too strong. The SAME
gate completed on the in-kernel GIC (A2 p50 58.82 x3; A3 p50 28.57
measured 2026-08-08 morning), and under the userspace GIC the A2 Vulkan
gate ALSO dies in a guest reset right after launch. Executing padding
through a dispatch slot is still a driver robustness bug, but the trigger
correlates with interrupt-delivery differences between hv_gic and the
userspace GIC (MSI-X latching, level-SPI re-fire after EOI, or IAR/EOI
ordering under load). Investigation continues with GIC-op tracing around
the crash.

## Impact on A3

- The D3D11 title gate cannot complete on the venus 120.45 driver: PPSSPP's
  D3D11 startup (viogpu_d3d10 UMD -> viogpu3d KMD) reliably bugchecks the
  guest within ~18 s of the gate command.
- The previous "A1-class park during the title gate" classification is
  retracted for A3: with boots stabilized, the failure is a reproducible
  guest bugcheck.
- Under the userspace GIC the A2 Vulkan gate also triggers a guest reset
  (a2-usgic2-170112: READY, staging OK, gate command sent, SYSTEM_RESET
  within ~20 s) -- so the trigger is not D3D11-specific under usgic.
- Fix path: trace the GIC operation stream at the crash (BRIDGEVM_USGIC_TRACE),
  compare MSI/INTx semantics against hv_gic, and fix the delivery term in
  the userspace GIC; the driver-side padding call remains a robustness bug
  worth a patch in the builder stack regardless.

## Artifacts

- Minidump: runs/a3-dmp-145824/share-host/080826-8828-01.dmp
- Gate runs: runs/a3-gate-142322, runs/a3-usgic-diag-*, logs /tmp/a3-usgic*.log
- Bugcheck events (WER + Kernel-Power 41) captured in-guest over the agent.
