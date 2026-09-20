# A9 WinPE receipt-comparison failure

## Scope

This record preserves physical T17 job
`t17-cdb6940c-source-lock-pilot-r13`. It does not promote A9, A11, the
product state, or any graphics claim. The job used the supported 3D-off path.

## Sealed run

- Exact main commit: `cdb6940ce0f8a87565127baa0ed0dfb20046ad79`.
- Hosted verification: all 45 exact-main push workflows passed; core CI run
  `35488319044` passed.
- App signing class: Apple Development, Team ID `CP9SP67S98`, CDHash
  `fd8bb984ad69171f840a45eb380a9c16eec81c43`.
- T17 input manifest SHA-256:
  `738ad9b27efe81345a512b68811d3034ccef2c80cdc9d1f7251a7b29ace8be08`.
- Verified input record SHA-256:
  `ea5d26c7616039e6f7fcf42d790aedd2b43282d67b4bf93f4812bd956436b500`.
- Strict public and private receipt SHA-256:
  `5790e48f2d51d5c63bdd70c7916fefdc99dc693d5c987d349286d3f26118ef52`.
- Authenticated private lane result SHA-256:
  `f2e8fb67a660f565f024f3a9e85a102c4c32cb11eb10dd3149f694d273675ea7`.

The receipt remains failed with `failure_code=installer-failed`, one artifact
preflight pass, one exact VM-creation pass, zero source-prepared passes, zero
Windows-installed passes, verified worker cleanup, and no claim or promotion.
The source cache was physically built and WinPE executed, but the official
source-prepared stage is false, so this record does not relabel that stage.

## Direct failure evidence

The final framebuffer shows DISM completing the Windows image apply at 100%,
then all three signed driver packages installing successfully. The next
command reports that `fc` is not recognized and the script emits
`BVINSTALL ERROR: guest provisioning receipt copy mismatch`. Therefore the
script exits before its `BVINSTALL BCDBOOT` line.

Read-only attachment of the retained 64 GiB target found the expected GPT,
260 MiB FAT32 ESP, 16 MiB MSR, and NTFS Windows partition. The ESP contained
neither `EFI/Microsoft/Boot/bootmgfw.efi` nor
`EFI/BridgeVM/install-success.txt`. This agrees with the displayed control
flow. The subsequent UEFI `bootmgfw.efi: Not Found` message is a consequence
of skipping `bcdboot`, not evidence of an NVMe or firmware-read defect.

The host log recorded 233 completed target writes and 13 completed flushes,
with no pending completion at the boundary. Its SHA-256 is
`bef477adf3794b5149d38d33271f7dbe80314f2d656bc4bce77ddc182e8b248c`.
That supports successful target I/O only; it does not prove installation.

## Repair and limits

Source `0caef866610399ef13836c3dee7b9821004697ef` replaces the unavailable
WinPE `fc` invocation with the fail-closed native byte comparator. App
packaging cross-compiles it as ARM64 Windows PE and bundles the result as an
exact install input. The release workflow installs that build-time compiler;
the runtime source builder injects the exact app-owned input into boot.wim
image 2 without requiring Zig, and comparison failure still stops
before `bcdboot`.

The focused wiring, security, source-builder and product-E2E contracts, all
four Swift shim suites, structural budgets, and the complete project check passed; log SHA-256 is `1069d530161254243a4200cdd3286a7ad2fe18ee8a31d6669ab67fc0ab7c7995`.
These deterministic results do not prove that the comparator executes in
WinPE or that the install reaches `bcdboot`; a new sealed physical pilot is
required. A9 and A11 remain OPEN.
