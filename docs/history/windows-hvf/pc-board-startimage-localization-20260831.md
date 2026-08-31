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
`GetVariable("Se…")` → EFI_NOT_READY; `HandleProtocol(LoadedImage)` → success;
`HandleProtocol` (GUID Data1 0x09576e91) → success; and
`AllocatePages(AllocateAddress, EfiLoaderData, at 0x102000)` → EFI_NOT_READY.
The board has no RAM at the low fixed address 0x102000 (system RAM starts at
0x1_0000_0000). The leading hypotheses for the spin are (a) a retry loop after
the fixed-address allocation fails, or (b) an architectural-timer/delay loop
that never completes because the board's timer contract does not match what the
Boot Manager expects. Pinning it needs single-stepping the spin region (rebuild
the tracer per the memory note). The extended watchdog was a local diagnostic
and is not committed.
