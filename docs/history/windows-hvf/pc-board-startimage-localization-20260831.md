# BridgeVM Virtual ARM PC — Windows Boot Manager StartImage localization

Date: 2026-08-31. Board: the from-scratch BridgeVM Virtual ARM PC on the HVF
engine (not the QEMU-compatibility engine). Classification: development
diagnostic record. No shipping-capability claim; the Windows-start gate (T14)
remains an honest FAIL and must not be reclassified until a live run reaches
`stage=7` with the handoff line.

## Symptom

The standards firmware boots to BDS, connects NVMe, finds one FAT ESP, and
`LoadImage` of `\EFI\BOOT\BOOTAA64.EFI` (Windows Boot Manager, `bootmgfw`)
succeeds with a valid `EFI_LOADED_IMAGE_PROTOCOL` (`ImageBase 0x27ee70000`,
`ImageSize 0x32b000`). `gBS->StartImage` then returns
`EFI_INVALID_PARAMETER (0x8000000000000002)` with empty `ExitData`, and a
re-probe of the image handle afterwards also returns `EFI_INVALID_PARAMETER`
(the handle was unloaded), i.e. the entry point ran and returned.

## What was ruled out (host-side, spec-based, fail-closed diagnostics)

- **GOP absence is not the cause.** A BridgeVM-owned reserved-RAM Graphics
  Output producer (1024x768 `PixelBlueGreenRedReserved8BitPerColor`) is present
  and enumerated (`gop_handles=1`); the failure is identical with it.
- **No failing UEFI service.** A boot-services recorder wrapping 19 entry
  points (including the foundational `RaiseTPL`/`RestoreTPL`/`CreateEvent`/
  `Stall`/`SetMem`/`CopyMem`) shows the Boot Manager issues **87 calls, all
  `RaiseTPL(TPL_HIGH_LEVEL)`/`RestoreTPL` pairs, every one `EFI_SUCCESS`**.
- **The StartImage mechanism is sound.** The board's own removable-media probe
  application starts and reaches post-`ExitBootServices` through the identical
  path (T13, 20/20). The failure is specific to `bootmgfw`.

## Single-step localization (decisive)

An HVF single-step tracer (host-side, no guest debugger) stepped the guest
from just before `StartImage` and recorded every program counter inside the
loaded image. Result:

- `bootmgfw` entry point = **image RVA `0x3a130`**.
- It executes 16 instructions, calls the function at **RVA `0xe9830`**
  (helpers at `0x2b1d50` and `0x2b1d70`); that function performs the 87
  successful `RaiseTPL`/`RestoreTPL` pairs and returns.
- Control then jumps to the **error epilogue at RVA `0x3aaf8`** and returns
  `EFI_INVALID_PARAMETER`. Only 77 in-image instructions run in total.

So the bail is **internal Boot Manager logic in the function at RVA `0xe9830`**,
which reads something the board provides directly — an ACPI table, an SMBIOS
structure, an EFI variable, or a memory attribute — through direct memory
access (not a boot service) and rejects it before doing any device or file I/O.

## Reproducing the tracer (it was not committed)

The tracer's firmware side is a store to a device-mapped arm address right
before `StartImage`; that store is instrumentation and must not ship, so the
tracer was reverted from the tree. To rebuild it:

- BDS stores to a **device-mapped** arm GPA — use an ECAM offset for a bus the
  board does not implement (e.g. `0x4FFFF000`). Do **not** use `0x30000000`:
  the firmware MMU leaves it unmapped, so the store faults into the guest and
  corrupts the run.
- The host, on the data abort at that GPA, sets `MDSCR_EL1.SS` (the HVF
  `hv_sys_reg` id for `MDSCR_EL1` is `0x8012`), sets `PSTATE.SS` (CPSR bit 21),
  calls `hv_vcpu_set_trap_debug_exceptions(vcpu, true)`, then on each
  `EC == 0x32` software-step exit records the PC and re-arms `CPSR.SS`. The
  `hv_sys_reg` encoding is `(op0<<14)|(op1<<11)|(CRn<<7)|(CRm<<3)|op2`
  (`MPIDR_EL1 == 0xc005` confirms it).

## Root cause (confirmed by disassembly)

Extracted `BOOTAA64.EFI` from the image ESP and disassembled it
(`aarch64-elf-objdump`; `.text` VA `0x1000` raw `0x400`; entry RVA `0x3a130`
confirmed; `SizeOfImage 0x32b000` matches the loaded image). `EfiEntry`
(`0x3a130`) calls the init at `0xe9830` with **`x0` = SystemTable**; that
function does `ldr x9,[x0,#64]` then `cbz x9,0xe9c24`. `SystemTable+64` is
**ConOut** (`EFI_SYSTEM_TABLE`: +64 ConOut, +88 RuntimeServices, +96
BootServices). The traced taken branch at `0xe9894` is exactly this `cbz x9`,
so **ConOut is NULL**, and `0xe9c24` returns the literal at `0xe9c58` =
`0x8000000000000002` (EFI_INVALID_PARAMETER).

**The Windows Boot Manager bails immediately when `gST->ConOut` is NULL.** The
boot firmware shipped no console-output producer, so ConOut was NULL.

## Resolution

Added `MdeModulePkg/Universal/Console/ConSplitterDxe` to the boot firmware. Its
entry installs the virtual console aggregators and sets `gST->ConOut` (and
ConIn/StdErr) non-NULL. With this in place a live run now reports
`diagnostic: COMPLETE` at `stage=7`: the Boot Manager enters and keeps running
(the run ends on the boot watchdog, not an immediate StartImage return). That
is the T14 handoff condition, and the sealed t8 B4 pointer-reliability gate
still lands 20/20 at p95 222 ms on the shipping engine at the same head, so the
console addition regresses nothing.

## Next wall (observed, not yet resolved)

Full Windows boot is still not proven (`windows_boot_proven=false`). A local
run with the observation watchdog extended to 150 s shows the Boot Manager
makes only six boot-service calls and then spins inside its own image (PC
ranges over image RVA 0x3f550..0x41838, an ~8 KiB loop) for the rest of the
window without reaching BCD/file I/O. The six calls are: RaiseTPL/RestoreTPL;
`GetVariable("Se…")` → EFI_NOT_FOUND; `HandleProtocol(LoadedImage)` → success;
`HandleProtocol` (GUID Data1 0x09576e91) → success; and
`AllocatePages(AllocateAddress, EfiLoaderData, at 0x102000)` → EFI_NOT_FOUND.
The board has no RAM at the low fixed address 0x102000 (system RAM starts at
0x1_0000_0000), but the Boot Manager tolerates that (AllocateAddress failing is
normal). The spin itself was localized by disassembling the loop:

- The loop body (near image RVA 0x3f540) calls a poll routine at RVA 0xc2698
  every iteration and keeps timeout counters (`[x22+3568]` incremented and
  compared to a limit `[x24+3576]`, calling 0x1e47a8 and resetting on expiry).
- 0xc2698 loads a global object at `[0x308000+2736]` and makes an indirect
  call through its vtable at offset +136 (`ldr x8,[x0]; ldr x8,[x8,#136];
  blr`) — it polls a method on a Boot-Manager object each iteration.
- The periodic handler 0x1e47a8 walks several objects (0x303000+0x960/0x7a0/
  0x860/0x820) through 0x1e57f0, i.e. it services a list on a timer tick.

It looked like a timed poll/event loop, so a **timer hypothesis was tested and
FALSIFIED**: the theory was that the runner never delivers a timer tick, so the
Boot Manager's timed wait never completes. Two experiments disproved it.

1. Instrumenting the runner shows **`vtimer_exits == 0`** — HVF's virtual-timer
   exit never fires, because the firmware uses the *physical* timer
   (`ArmGenericTimerPhyCounterLib`, `PcdArmArchTimerIntrNum=30`).
2. Switching the firmware to the *virtual* timer
   (`ArmGenericTimerVirtCounterLib`) AND adding the runner-side delivery
   (`hv_vcpu_set_pending_interrupt(IRQ)` + unmask on `EXIT_VTIMER`, mirroring
   the shipping engine's `service_windows_arm_firmware_vtimer_delivery`) left
   the spin **byte-for-byte identical** (same final PC `0x27feb1838`, same six
   service calls). Both experiments were reverted; nothing timer-related is
   committed.

So the spin is **not** timer-driven. It was then localized further by dumping
guest RAM (a temporary, reverted diagnostic that reads the globals through the
loaded image base): `[0x308000+2744]` and `[0x308000+2736]` are both **0**, so
the `0xc2698` vtable+136 indirect call is never reached (it is guarded by
`cbz` on exactly those null globals). The vtable-poll theory is therefore also
ruled out.

The real spin is at image RVA **0x41838**: `mov x0,#0; bl 0x42500; cmp x0,#1;
b.ne …` — the Boot Manager repeatedly calls **0x42500** and loops while the
result is not what it awaits. 0x42500 queries an internal subsystem: it calls
0x1ae9c8 (returns a context), loads `[ctx+24]`, and calls 0x1b0a20 / 0x1b0b60
with GUID/key-like word constants (0x16000020, 0x250000c2, 0x25000008); it
returns 1 only when a global `[0x2ef000+1632]` is null. Nearby helper 0x3f600
reads **`PMCCNTR_EL0`** (the PMU cycle counter) for elapsed-time math, so the
PMU cycle counter is also in play for this phase's timing.

**Corrected next step:** capture registers at run time at 0x42500 (single-step
or a hardware breakpoint with register dump) to see which key/context it queries
through 0x1ae9c8/0x1b0a20 and what value it keeps getting, and confirm whether
`PMCCNTR_EL0` advances for the guest on this board (if the PMU cycle counter is
frozen at 0, cycle-based elapsed-time math never progresses). Both the
vtable-object poll and the UEFI-timer-tick theories are already disproven; do
not re-try them.

Note on tooling: HVF's `hv_vcpu_get_sys_reg` does not expose PMCCNTR_EL0,
CNTVCT_EL0, PMCR_EL0 or PMCNTENSET_EL0 (all return an error), so the host
cannot sample them directly — the guest reads them with `mrs`, so confirming
whether PMCCNTR advances needs a single-step capture of the `mrs` result, not
a host-side `get_sys_reg`.

Single-step attempt and its limits (2026-08-31): the tracer was rebuilt and
run against the ConOut firmware. Two lessons. (1) In single-step mode HVF's
`hv_vcpus_exit` from the watchdog thread does **not** break the step loop —
each `hv_vcpu_run` returns a software-step exit first — so the tracer MUST
carry its own step budget or it hangs forever (a first run without one had to
be killed after an hour). (2) Single-step is ~3000 steps/s, so a 300k-step
budget covers only the first ~300k guest instructions after StartImage — the
Boot Manager's entry and the start of its main loop — and does **not** reach
the native-speed steady state (the 20s terminal PC 0x41838 is not in the
captured set). The captured 363 unique in-image RVAs show an **active
multi-function loop** (main body ~0x3a854..0x3ad d4 calling helpers at
0x53504..0x536d0, 0x36490..0x366a0, 0x1e42d0..0x1e4378), and ~299k of the 300k
steps were *out of image*, i.e. the Boot Manager is executing a lot of code
outside its own image while only six wrapped boot-services calls are recorded
— it is doing work, not sitting in a one-instruction spin.

**Hardware-breakpoint attempt (also a dead end for now):** a breakpoint probe
was built — arm store, then `DBGBVR0_EL1 = ImageBase+RVA`, `DBGBCR0_EL1`
enabled (E, PMC=0b11, BAS=0b1111), `MDSCR_EL1.MDE`, `trap_debug_exceptions` —
and it **never fired**, even at 0x41838 which is definitely executed (it is the
native-speed terminal PC). Combined with `get_sys_reg` not exposing PMCCNTR/
CNTVCT, this indicates **HVF's `set_sys_reg` does not honor the debug registers
(DBGBVR0/DBGBCR0/MDSCR.MDE) for guest hardware breakpoints** — only software
single-step (MDSCR.SS + trap_debug, EC 0x32) works, and that is too slow to
reach the native-speed steady state. So neither of HVF's obvious guest-debug
primitives can capture registers at 0x41838 as-is.

**Revised tooling for next time:** the remaining viable options are (a) patch a
`BRK #imm` into the loaded image in guest RAM at the target RVA before
execution (a software breakpoint that traps as EC 0x3c, self-inflicted and
reliably delivered) and read registers at that exit; or (b) run software
single-step for far longer (dedicated long job) to reach steady state. Option
(a) is the cleaner next step. Always give any debug-exception loop a
self-terminating budget — single-step swallows the watchdog's hv_vcpus_exit.
