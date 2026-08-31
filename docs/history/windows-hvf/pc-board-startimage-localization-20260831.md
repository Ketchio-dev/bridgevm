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

**BRK-patch attempt (also a dead end):** the BRK-in-guest-RAM probe was built —
at arm time (before the target RVA first executes) the host reads the original
4 bytes, writes `BRK #0` (`0xd4200000`), enables `trap_debug_exceptions`, and
waits for `EC == 0x3c`. It **never fired** either. The guest instruction/data
caches are on (SCTLR.C/I set by the board MMU), and a host write to the
HVF-mapped RAM does not invalidate the guest caches, so the guest keeps
fetching the original instruction — host RAM patching of guest code is not
cache-coherent here.

**On-disk BRK/HVC patch attempt (a fourth path, also blocked for now):** the
Boot Manager's on-disk `bootaa64.efi` was patched on a COW clone — file offset
0x40c38 (which held `0x94000332` = `bl 0x42500`, confirming RVA 0x41838) was
overwritten with `hvc #0` (`0xd4000002`), so EDK2 would load it with cache
maintenance and the trap would actually execute (HVC always routes to EL2). The
run still ended on the watchdog with PC at 0x41838 executing as the original
`bl`, i.e. the patch did not reach the loaded image — the write through the
`hdiutil`/macOS msdos mount did not persist into the `.raw` before the run. To
make this work, patch the `.raw` at the **physical** byte offset of that file
cluster (parse the FAT to resolve it) instead of writing through a mounted
volume, and re-confirm the loaded bytes. The run did capture x0-x30 at 0x41838
at the watchdog boundary: x0=0, x1=0, x2=0x4f, x19=image+0x142000,
x30(LR)=image+0x3f56c (so 0x41838's function is called from ~0x3f568, the loop
seen earlier), x6=0x70536f54 (ASCII "ToSp"). Not decisive on its own.

A follow-up patched the `.raw` at the **physical** FAT-cluster offset
(bootaa64.efi start cluster 5; file offset 0x40c38 resolves to raw offset
0x543c38; the original bytes there were `0x94000332` = `bl 0x42500`, confirming
the location). The write was verified in the `.raw`, and `boot_media` reads the
`.raw` directly, yet the guest STILL ran 0x41838 as the original `bl`. The most
likely explanation is **Boot Manager self-relocation**: bootmgfw re-copies /
re-decompresses its own image after load and overwrites 0x41838 with the
original instruction from an internal source that has no patch, so a static
patch of the loaded image (whether in RAM or on disk) is defeated. Intercepting
it would need the patch applied *after* that self-copy, i.e. real runtime debug
control, which HVF does not provide here.

**All four guest-debug tooling paths are therefore exhausted** under HVF's
constraints as attempted: software single-step works but is too slow to reach
the native-speed steady state; hardware breakpoints are unavailable (HVF
ignores the debug sys-regs); host RAM code-patching is defeated by guest
caches; and static image patching (RAM or on-disk) is defeated by Boot Manager
self-relocation. Net: register-level inspection of the native-speed steady-state
spin is not achievable with the tooling available in this session. The
productive direction is therefore (b) — stop per-wall RE and build out the
board environment (a real GraphicsConsole on the GOP, a working ConIn, and the
device stack) so the Boot Manager runs against a fuller platform — rather than
more static probing.

## Breakthrough: a real GOP console advances the Boot Manager (commit c6a8dcc3)

Direction (b) paid off immediately. The ConSplitter virtual console had set
`gST->ConOut` non-NULL but as a no-op sink, so the Boot Manager spun early
(only 6 boot-service calls, at 0x41838). Adding **HiiDatabaseDxe** (the HII
Font protocol) and **GraphicsConsoleDxe** (text rendering on the GOP
framebuffer), and then pointing `gST->ConOut`/`StdErr` at the Simple Text
Output that GraphicsConsole installs on the GOP handle (with a system-table CRC
reseal, in BdsConsole.c) gives the Boot Manager a console that actually draws.
With it, a live run jumps from **6 to ~4000 boot-service calls** and the
terminal PC moves to a **different region (image RVA 0x19550)** — the Boot
Manager now runs far past the old spin. It still ends on the watchdog
(`windows_boot_proven=false`), so there is a later wall, but this confirms the
console was the real gap and that building out the board environment is the way
forward.

Filtering RaiseTPL/RestoreTPL out of the trace (a reverted diagnostic) shows
the Boot Manager now makes ~60 *meaningful* boot-service calls before it stalls:
`GetVariable("Se…")` → NOT_FOUND, two `HandleProtocol`, then ~56 `AllocatePool`
(EfiBootServicesData: many 0x13-byte structures, then 0x4d8 and 0x6620
buffers), then one `AllocatePages` — after which the terminal PC is
**0x27fe89550, which is ABOVE the loaded image** (ImageBase 0x27ee70000 +
SizeOfImage 0x32b000 = 0x27f19b000). So the Boot Manager has moved out of its
static image and is executing code in memory it allocated/loaded (a relocated
or decompressed module), i.e. real forward progress, not the earlier
static-image spin. Because this later stall is in runtime-loaded code, static
disassembly of bootaa64.efi cannot reach it.

Delivering the timer (switching the firmware to the virtual timer and injecting
`hv_vcpu_set_pending_interrupt(IRQ)` + unmask on `EXIT_VTIMER`, mirroring the
shipping engine) did **not** change this later stall either — same ~60 calls,
same region — so, like the first spin, it is not timer-driven; that experiment
was reverted. Next: to look past this wall, either dump/patch the runtime-loaded
code region (0x27fe8xxxx) rather than the static image, or keep building out the
board (a working ConIn / keyboard, and the storage/file path the Boot Manager
will need to read the BCD) — building out the environment is the proven lever.
Inspecting registers at the steady-state spin (0x41838) is not feasible with
any of them as-is. Genuinely different options for a future session: patch the
`BRK` into the **on-disk** `bootaa64.efi` on the per-run COW clone (EDK2 loads
it with proper cache maintenance, so the trap instruction is actually
executed — but confirm the BRK routes to the VMM under `trap_debug_exceptions`
rather than to the guest's own EL1 vector, and note this needs the stub
SecurityDxe to skip signature checks, which it does); or step back from
per-wall bootmgfw RE toward building out a more complete board environment.

## Breakthrough 2: wiring ConIn advances the Boot Manager into a live timer-wait loop

Building out the board environment (the proven lever above) paid off again.
The console fix wired `gST->ConOut` but left `gST->ConIn` **NULL** — the exact
symmetric hazard as the original blocker (a boot application that touches the
console then dereferences a NULL pointer). ConSplitter installs a virtual
Simple Text Input aggregator handle even with no physical keyboard, so
`BdsConsole.c` now also locates a Simple Text Input producer and points
`gST->ConIn`/`ConsoleInHandle` at it (single CRC reseal covering both). The
Boot Manager's INF gained `gEfiSimpleTextInProtocolGuid`.

Effect (live, `win25h2-scripted-source.raw` COW, firmware `82a6fa3b`): the Boot
Manager no longer dead-stalls after ~56 `AllocatePool`. It now runs a **live
RaiseTPL(0x1f)/RestoreTPL(0x4) polling loop of thousands of iterations** —
i.e. it is executing, not stuck — before the 20s watchdog cancels the vCPU. The
sealed T13 BDS/ExitBootServices path is **unchanged (still PASS, stage=11)**, so
this is pure forward progress with no regression.

### The next blocker is now precisely localized: the physical-timer interrupt

The terminal state decodes the next wall exactly. `vcpu_final` is
`pc:0x27fe89550, esr:0x6234f804`. ESR `EC = 0x18` (trapped MSR/MRS), and the ISS
decodes to `Op0=3 Op1=3 CRn=14 CRm=2 Op2=2`, direction=write — that is a **write
to `CNTP_CVAL_EL0`**, the *physical* timer compare register. The Boot Manager
(through the firmware's TimerArch/event services) is arming the physical timer
inside its RaiseTPL/RestoreTPL wait loop and spinning until the timeout event
fires. It never fires: the board uses the **physical** timer
(`ArmGenericTimerPhyCounterLib` + `ArmTimerDxe` programming `CNTP_*`), HVF traps
the `CNTP_CVAL_EL0` write (hence EC 0x18), and the run loop masks `EXIT_VTIMER`
and injects **no** physical-timer PPI — so the guest's timed wait can never
complete. (The earlier "timer delivery didn't help" experiment is not a
counter-example: it was run at the *old* AllocatePool stall, before ConIn let
the guest reach this timer-arming loop at all.)

Next: give the board a working architectural timer interrupt — the cleanest
route is to switch the firmware to the **virtual** timer (`CNTV`, which HVF
supports natively via `EXIT_VTIMER` + IRQ injection) and stop masking vtimer in
the run loop, so the Boot Manager's timed wait completes and it proceeds toward
reading the BCD. This is again "build out the board," now the timer path.

## Breakthrough 3: the virtual timer makes the board interrupt-driven

Two coordinated changes gave the board a live architectural timer:

- **Firmware**: the DSC `ArmGenericTimerCounterLib` mapping was switched from
  `ArmGenericTimerPhyCounterLib` to `ArmGenericTimerVirtCounterLib`, so the whole
  timer stack (TimerLib, `ArmTimerDxe`) now reads `CNTVCT` and arms `CNTV_CVAL`
  instead of the physical `CNTP_*`. `ArmTimerDxe` already registers the virtual
  PPI (INTID 27, `PcdArmArchTimerVirtIntrNum`). Only three module hashes moved
  (`ArmGicV3Dxe`, `ArmTimerDxe`, `Metronome`); firmware rehashed to
  `45e18b5d…`.
- **Run loop**: `EXIT_VTIMER` used to *mask* the vtimer as "unexpected" and
  never unmask it, so the very first tick silenced the timer forever. It now
  *unmasks* on the activation exit (HVF auto-masks there), letting the in-kernel
  `hv_gic` keep delivering INTID 27. The vcpu already unmasks the vtimer once at
  startup.

Live effect (`win25h2` COW, firmware `45e18b5d`): the Boot Manager's terminal
ESR changes from the `CNTP_CVAL_EL0` write to **`esr=0x62323018` = a write to
`ICC_EOIR1_EL1`** (ISS `Op0=3 Op1=0 CRn=12 CRm=12 Op2=1`), the GIC
End-Of-Interrupt register. The guest is now **taking and acknowledging
interrupts** — the timer tick is delivered and serviced. Boot-service activity
roughly **doubles** (max trace seq ~4059 → ~8059 in the same 20s window). The
sealed T13 BDS/ExitBootServices path is **unchanged (still PASS, stage=11,
`vtimer_exits=0`** — it completes before a tick is even needed), so no
regression.

### The next frontier: an input/event source

Extending the watchdog to 75s shows the guest holds a **stable steady state**:
same PC region (`0x27fe8956x`), same `ICC_EOIR1_EL1` ESR, and the trace seq
grows linearly (~8059 at 20s → ~30059 at 75s, ~400 calls/s), entirely
`RaiseTPL(0x1f)`/`RestoreTPL(0x4)`. That is a `WaitForEvent`/`CheckEvent`-style
busy wait, now driven by the timer, that never completes because the event it
waits on never arrives. With `ConIn` wired but **no key ever delivered**, the
most likely event is a keystroke at a boot menu / prompt (or an IO-completion
event). So the next build-out is an **input path** that can actually inject a
key into `ConIn` (a synthetic key source or a keyboard device), or identifying
the exact event set the Boot Manager polls here. This is a genuinely later
frontier than the dead-timer stall — the board now runs interrupt-driven.

### Diagnostic: the wait is an event wait, not an IO poll (input, not storage)

A run-loop MMIO histogram (example-only, reverted) settles the input-vs-IO
question. Over a full 20s Windows run the guest issues only ~1500 MMIO accesses
total — `pcie_ecam` ~1267 and 64-bit BAR ~208, and **zero** `pcie_mmio32` or GIC
distributor/redistributor MMIO — while boot-service activity reaches ~8000
calls. So the RaiseTPL/RestoreTPL wait loop does **no per-iteration MMIO**: it is
a CPU/sysreg event wait (timer ticks acknowledged through the GIC `ICC_*`
system registers), not a device poll. And the board's NVMe is **firmware-polled**
— EDK2's `NvmExpressDxe` polls the completion-queue phase bit (which is how the
sealed T13 probe reads `BOOTAA64.EFI` with no interrupt), so the Boot Manager's
file reads never block on an interrupt either. Both rule out a storage/IO wait
and point the next build squarely at an **input/event source** (a synthetic key
into `ConIn`).

## Breakthrough 4 (partial) + a reframing: the wait is an early-init spin

Two changes were built to test the input hypothesis and to see past the wait:

- **A synthetic `ConIn` key source** (`BdsInput.c`): a minimal Simple Text Input
  whose `WaitForKey` event is always signaled and whose `ReadKeyStroke` yields
  Enter, installed on its own handle with `gST->ConIn` repointed at it, right
  before `StartImage`. A bring-up scaffold (a real HID keyboard replaces it
  later).
- **Filtered the trace ring**: `RaiseTPL`/`RestoreTPL` are no longer recorded
  (the wrapper stays installed for ExitBootServices symmetry), so the bounded
  126-entry ring shows the *meaningful* calls instead of drowning in the wait
  loop's TPL churn.

Result — the synthetic Enter key changes **nothing** (identical terminal PC,
ESR, and ~8000 call count), and the filtered trace explains why: the Boot
Manager's meaningful boot-service calls **stop at just 60** — one `GetVariable`
(NOT_FOUND), two `HandleProtocol`, 56 `AllocatePool` of 0x13 bytes, then a 0x4d8
and a 0x6620 buffer, one `AllocatePages` — and then the RaiseTPL/RestoreTPL loop
runs forever. There is **no `CreateEvent`, no `Stall`, no `WaitForEvent`-implied
event, no `LoadImage`, no filesystem open**. So the Boot Manager is stuck **very
early**, before it ever reads the BCD or a file — not at a late boot menu — and
because it created no events of its own, this is not `WaitForEvent` on its own
events but a spin in its own code that `RaiseTPL(HIGH)`/`RestoreTPL(APP)` drives
(each `RestoreTPL` dispatches the firmware's queued timer notifies). The virtual
timer makes the guest service those interrupts (`ICC_EOIR1_EL1` writes) but that
does not satisfy whatever condition the spin polls.

This reframes the frontier: it is **not** an interactive keystroke prompt (the
synthetic key disproves that) and **not** storage IO (the histogram/polled-NVMe
argument). It is an early-init spin in the Boot Manager's runtime-loaded code
(`0x27fe8956x`, above the static image), so the decisive next tool is
**guest-side symbolication of that runtime-loaded region** — dumping and
disassembling the code at the spin PC (the crash-survivable RAM-dump technique
used elsewhere) to name the exact loop and the value it polls. The sealed T13
BDS/ExitBootServices path stays **PASS, stage=11** through both changes.

## Breakthrough 5: the spin is "wait for the tick counter to advance"

A guest RAM dump at the terminal boundary (an env-gated dump added to the
example — `BRIDGEVM_PC_DUMP=path` writes a window of `guest_ram.bytes()` around
`state.pc`; reverted to keep the harness lean, trivially re-added) disassembles
the spin exactly. Because the Boot Manager runs identity-mapped in boot
services, the spin GVA `0x27fe8956x` is also its GPA, at offset `0x17fe8956x`
into RAM (`RAM_BASE 0x1_0000_0000`). The loop is:

```
27fe89534:  stp x29,x30,[sp,#-48]!      ; function entry
27fe89540:  adrp x19, 0x27fe8c000       ; x19 = module data page
27fe89544:  ldr  x0, [x19, #216]        ; x0 = *(0x27fe8c0d8)  (initial)
27fe89548:  str  x0, [sp, #40]          ; snapshot it
27fe8954c:  ldr  x1, [sp, #40]          ; x1 = snapshot
27fe89550:  ldr  x0, [x19, #216]        ; x0 = *(0x27fe8c0d8)  (current)
27fe89554:  cmp  x1, x0
27fe89558:  b.eq 0x27fe89568            ; while unchanged:
27fe89568:  bl   0x27fe8b838            ;   pump()
27fe8956c:  b    0x27fe8954c            ;   loop
            ; (falls through to ret only once the value changes)
```

`0x27fe8b838` — the function called every iteration — is `nop;nop;nop;nop;nop;
ret`, i.e. a pure CPU-relax stub. So the Boot Manager snapshots a global word at
its data page + 0xd8 (`0x27fe8c0d8`) and **busy-waits for that word to change**,
doing nothing but relax. That word is a tick/time counter the Boot Manager
expects a handler to advance. It never advances, so the spin never exits — even
though the virtual timer fires and the guest services interrupts
(`ICC_EOIR1_EL1` writes). The wall is therefore precisely: **the Boot Manager's
own tick counter is not being incremented despite interrupt servicing.** The
focused next step is to find the writer of `[0x27fe8c000 + 0xd8]` (disassemble
the module for a `str` to that slot — its timer ISR / notify path) and determine
why the delivered interrupt does not reach it: most likely the INTID the guest
reads from `ICC_IAR1_EL1` is not the timer PPI the Boot Manager's handler
expects, or its timer is armed on a source the in-kernel GIC is not routing to
that handler. This is a single, well-scoped guest-GIC question, not a broad
search.

### Deeper read: the counter is external-writer-only and frozen at zero

Dumping the whole spinning module (1 MB from `0x27fe00000`) and disassembling it
pins the counter's role and rules out an in-module writer. At the terminal
boundary `*(0x27fe8c0d8) == 0`, and it stays zero across the 20s and 75s runs —
it never advances. Its neighbors in that data page are function pointers and
config (`0x27fe8c0c8 = 0x27fe892ec`, `0x27fe8c0e0 = 1`), i.e. it lives in a
module state struct. The spin function `0x27fe89534` is called from one helper
(`0x27fe8a0f8`) that formats a message (calls a `vsnprintf`-like routine at
`0x27fe895c4`, with the format strings that sit right after the `nop`-pad at
`0x27fe8b850`) and invokes a sink callback through a function pointer — this is
the Boot Manager's **diagnostic/timestamped-logging path**, and the spin is its
"wait for the timestamp/tick to advance" step.

Crucially, **no instruction in the 1 MB module writes `0x27fe8c0d8`**: none of
the 18 `adrp`s to its page `0x27fe8c000` is followed by a store to offset `0xd8`,
and the only `str …,[x,#216]` sites use unrelated base registers. So the counter
is advanced by code in **another image** — an interrupt handler (the timer tick)
that lives outside this module and is not running. This confirms the wall is an
**interrupt-delivery gap**, not anything in the Boot Manager's own module: the
guest services *some* interrupt (`ICC_EOIR1_EL1` writes, most likely the
firmware's own virtual-timer tick) but the handler that would bump this counter
never runs. The focused next step is to identify that handler's interrupt source
(the Boot Manager installs its own `VBAR_EL1` and may arm the **physical** timer,
which HVF does not deliver to the in-kernel GIC — only the virtual timer is
native) and route/emulate it so the tick handler runs and the counter advances.
