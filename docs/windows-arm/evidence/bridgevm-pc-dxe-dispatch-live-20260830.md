# BridgeVM Virtual ARM PC: DXE Core dispatches a BridgeVM driver (2026-08-30)

## Evidence rank and result

This is a **live single-run** receipt, not a fixed-sample product gate. At exact
BridgeVM code head `183571b5a0d60d44d84e9eb7b62b6d5d69959b0e`, the
development-only BridgeVM firmware entered a pinned generic DXE Core and that
core dispatched a BridgeVM-owned marker driver on a real `Mac17,9` host running
macOS 26.5:

```text
BridgeVM Virtual ARM PC DXE-dispatch probe: PASS
board=com.ketchio.bridgevm.virtual-arm-pc abi=1
reset_vector=0x0 firmware_size=0x4000000 ram=0x100000000
sec_result=1 hob_count=7 hob_list_gpa=0x100004000 hob_list_size=272 dxe_result=8 system_table=0x11ffc0018 system_table_signature=0x5453595320494249 result_gpa=0x100001000 boot_info=0x26000000
firmware_sha256=57c134b8f3f42bb9bb020936d4d87926b0d6563bfa0339bb110996a6e4ed6da6
LIVE PROOF: DXE Core created the UEFI system table and dispatched the BridgeVM probe
```

The exact command was:

```sh
BRIDGEVM_HVF_ALLOW_LIVE_BRIDGEVM_PC_DXE_ENTRY=1 \
  tests/integration/bridgevm-pc-dxe-entry-live-opt-in-smoke.sh
```

The host had booted at 2026-08-30 11:34:29 EDT. The ad-hoc-signed debug
example carried only the Hypervisor.framework entitlement and had post-signing
SHA-256
`305b501089b8ec7efc8423e21ca94ce4dfc112d824b0d6e93e5dcd009bb48af4`.
This binary is a local probe receipt, not a redistributable artifact.

GitHub-hosted CI run `33337724436` failed at the exact live-evidence head
because the new example's two tests were not included in the reachability
suite. Security and quality run `33337725671` passed. The omission was fixed
without changing the guest binary. At final code head
`53d2fa832de07f38a65a730992fa90c549e31e4c`, CI run `33338225556` and
Security and quality run `33338226989` both passed.

## What ran

The reset path installed a bounded EL1 exception vector, enabled EL1 FP/SIMD,
validated boot-info v1 and constructed the previously proven five-entry PI HOB
list. The DXE IPL continuation then validated the embedded firmware-volume and
PE/COFF headers, appended an FV HOB and a DXE-core module-allocation HOB, and
copied the fixed-rebased DXE Core image to `0x1_0040_0000` with explicit data
and instruction-cache synchronization.

The resulting 272-byte list contained seven HOBs. The host independently
validated the complete prior PHIT/resource/stack/CPU sequence plus:

| Offset | HOB | Length | Proven contents |
|---:|---|---:|---|
| `168` | firmware volume | 24 | base `0x0010_0000`, length 1 MiB |
| `192` | module allocation | 72 | standard module-allocation GUID, DXE Core GUID, base `0x1_0040_0000`, size `0x17000`, entry `0x1_0040_6bec` |
| `264` | end | 8 | standard end-of-list type and zero reserved field |

DXE Core created an EFI system table, discovered the BridgeVM DXE probe in the
firmware volume and called it. The probe wrote stage `8` and the system-table
pointer to guest RAM, issued a data synchronization barrier and exited through
HVC. The host then dereferenced that pointer inside mapped guest RAM and
required the standard `EFI_SYSTEM_TABLE_SIGNATURE` value
`0x5453595320494249`; a stage marker alone could not pass.

The reproducible development FD is 64 MiB with SHA-256
`57c134b8f3f42bb9bb020936d4d87926b0d6563bfa0339bb110996a6e4ed6da6`.
Pinned GCC 16.1.0 and GNU binutils 2.46.1 produced a 6,144-byte
reset/SEC/HOB/IPL/exception entry with SHA-256
`a8d8a79279903253dd7dcc4d34a43aa5c00ac597cf45db613c9d23f03c69ddba`.
The exact build also recorded:

- EDK2 commit: `b03a21a63e3bd001f52c527e5a57feddb53a690b`
- fixed-rebased DXE Core SHA-256: `b4ca5c00ef7e1b4104776005fe3c07978c78e39d92d2f035cfa72edabdf77d10`
- BridgeVM DXE probe SHA-256: `463912d8120a00dbcf1cc2493857b318b092889fcd473df53fc1bfa363c4afac`
- combined FV SHA-256: `181e8f906e16e412afbd4fae9fd418e5f322ca5a823db1b7f60a737b9916a413`
- BridgeVmPcPkg source-tree SHA-256: `e3eee98e48a03a652483212ad7db5fd05a7a6588300209a17d72aec350a6eaa8`
- build receipt SHA-256: `b4a84251678a94fa98d9bf059ddc532c1b22a551a73bf71a5b2146563380ea3f`

The earlier reset/SEC/HOB FD remained byte-identical at SHA-256
`8db976249ff86c9613d0a13a0d811ee68a94ef90835d524deb451b927bc332d6`
and its exact live regression passed after the DXE-only linker path was
separated.

The final code-head cleanup restored the source-level stack-contract symbol
and made the boundary guards work on the Studio queue's minimal PATH. A pinned
rebuild at `53d2fa832de07f38a65a730992fa90c549e31e4c` produced the same
6,144-byte vector, DXE components, firmware volume and 64 MiB FD hashes listed
above. Its updated source-tree SHA-256 was
`16ba344438aa8aead0eb7ae29d9314c1e9c632afe333f84bd908b4e7edb59e7d` and
build-receipt SHA-256 was
`d2952cf4c3a3e8657a21f1837bb53bba44e190c1da7537e71e146fb02c165db9`.
Studio t0-check job `20260830-180625-48878-9087` then passed every code and
test step, including 2,110 reachable tests and the firmware boundary; its sole
failure was the intentionally stale capability registry before the A11
docs-only seal. The check-log SHA-256 was
`a73a78eab70577913371930c60c5f76e9a6fdebdb52bb6580f0fc618bc1b4ec3`.

## Failed candidates retained

Several live candidates returned through the IPL sentinel while the host still
saw the old five-HOB result. Adding a distinct HVC immediate proved that this
was not the BridgeVM probe. Reading the HVC arguments produced an invalid IPL
return value `5`; mapping the full advertised 512 MiB did not change it, so the
initial insufficient-RAM explanation was refuted.

A fail-closed EL1 exception vector then exposed the actual first error:
`ESR_EL1=0x96000021`, `ELR_EL1=0x6d8`, `FAR_EL1=0x100f1c`. The IPL had made an
unaligned 64-bit C load from the PE Optional Header while alignment checking
was enabled. Replacing it with two aligned 32-bit loads advanced execution and
exposed the next error, `ESR_EL1=0x1fe00000` at
`ELR_EL1=0x100412808`. Disassembly identified `stp d8, d9`, proving an FP/SIMD
access trap; enabling `CPACR_EL1.FPEN` at reset removed it. Only the subsequent
candidate dispatched the probe.

One later regression attempt changed the legacy reset/HOB FD to
`6595b83cbcab79f5b9aff4571589092b2b20959fa5322cb0140dee926d34e19c` by
placing DXE-only vector layout and synchronization in the shared path. That
candidate was rejected. The DXE linker and final barrier are now conditional,
and the original reset/HOB hash and live result are restored.

## Claim boundary

This result proves one reset-to-DXE-dispatch run on Hypervisor.framework. It
also proves that this bounded path created an EFI system-table object with the
standard signature. It does **not** prove complete UEFI boot or runtime
services, architectural protocols, variables, GOP, block I/O, a boot manager,
Windows installation or Windows boot.

Device, security and fixed-sample reliability gates for the independent board
remain open. The current product still uses its existing QEMU `virt`-compatible
guest contract, and A9 remains OPEN.
