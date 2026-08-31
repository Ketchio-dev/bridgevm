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

## Next step

Disassemble `bootmgfw.efi` at RVA `0xe9830` / `0x2b1d50` / `0x2b1d70` (extract
the PE from the image's ESP, or dump guest RAM at `0x27ee70000` after load) to
identify the exact value it reads and rejects, then make the board provide a
conforming value. Until a live run reaches `stage=7`, the Windows-start gate
stays an honest FAIL.
