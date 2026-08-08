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
This is a guest-driver defect (bad function-pointer slot at +0x250 of some
dispatch table in the venus-WIP viogpu3d), not a VMM defect: the same
driver ran the same path to the same instruction on both boots.

## Impact on A3

- The D3D11 title gate cannot complete on the venus 120.45 driver: PPSSPP's
  D3D11 startup (viogpu_d3d10 UMD -> viogpu3d KMD) reliably bugchecks the
  guest within ~18 s of the gate command.
- The previous "A1-class park during the title gate" classification is
  retracted for A3: with boots stabilized, the failure is a reproducible
  guest bugcheck. A2 (Vulkan path) PASSED on this same driver -- the defect
  is specific to a D3D10/11-path dispatch table.
- Fix path: locate the +0x250 slot in the viogpu3d dispatch object
  (builder repo patch stack), correct the table, rebuild via the GHA
  builder, restage. Tracked as the single remaining A3 blocker.

## Artifacts

- Minidump: runs/a3-dmp-145824/share-host/080826-8828-01.dmp
- Gate runs: runs/a3-gate-142322, runs/a3-usgic-diag-*, logs /tmp/a3-usgic*.log
- Bugcheck events (WER + Kernel-Power 41) captured in-guest over the agent.
