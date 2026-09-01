# BridgeVM PC UEFI BDS and ExitBootServices, fixed N=20 (2026-08-31)

This is development evidence for the experimental BridgeVM Virtual ARM PC. It
does not change the shipping Windows board or promote the product state.

## Exact-head re-seal after vTPM fixture cleanup

Studio tier `t13-bridgevm-pc-bds-exit` job `t13-9d63f07d-n20` repeated the
unchanged fixed N=20 gate on exact tested code head
`9d63f07dccf8399934baa640c6ebce72f28731d6`. All 20 independent disk-and-vars
lanes passed, with zero failed lanes and no known confounders. The public
[`9d63f07d re-seal receipt`](bridgevm-pc-bds-exit-reseal-receipt-20260901-9d63f07d.json)
has SHA-256
`034b6c20d2b2a0d9833cefe0f25e38647f8c2c537af777cd959c7ef3f1d3ecc5`.

The rebuilt development FD has SHA-256
`55a0aa3bff005ab6afa4b39ffbfaf239ffe12084aebd40f1202c87d1593fd8d3`;
the ESP image remained
`a49be97db44c0d68b3382f3b1e46eba2fc7a3b12bcba14c1ec720f0511b71979`.
GitHub-hosted Security and quality run `33548294709` passed at that exact code
head. CI run `33548294700` passed every code, build and test job and failed
only the freshness job because the registry still named the preceding tested
head; this new receipt exists to close that fail-closed transition. No result
is promoted until the subsequent full-history seal CI passes.

## Exact-head re-seal after descriptor and queue-runtime hardening

Studio tier `t13-bridgevm-pc-bds-exit` job `t13-77d86a77-n20` repeated the
unchanged fixed N=20 gate on exact tested code head
`77d86a77fb93f53df9cbdfbe3c3368286eedbbe4`. All 20 independent disk-and-vars
lanes passed, with zero failed lanes and no known confounders. The public
[`2026-09-01 re-seal receipt`](bridgevm-pc-bds-exit-reseal-receipt-20260901.json)
has SHA-256
`ddbe6930db61c07df3ca59ab0e39224e9c18684fdbcc67860631fe4eb91f2e7b`.

The rebuilt development FD has SHA-256
`55a0aa3bff005ab6afa4b39ffbfaf239ffe12084aebd40f1202c87d1593fd8d3`;
the ESP image remained
`a49be97db44c0d68b3382f3b1e46eba2fc7a3b12bcba14c1ec720f0511b71979`.
GitHub-hosted CI run `33543866744`, Security and quality run `33543866629`
and Windows ARM64 UMD run `33543866711` all passed at that exact tested code
head. This re-seal does not promote A9 or the product state.

## Exact-head re-seal after the modeled Windows runtime changes

Studio tier `t13-bridgevm-pc-bds-exit` job
`20260831-215833-85415-10446` repeated the unchanged fixed N=20 gate on exact
tested code head `772a587c08c17df6c8a99acd0b7db4df25e2d5c1`. All 20 independent
disk-and-vars lanes passed, with zero failed lanes and no known confounders.
The public
[`re-seal receipt`](bridgevm-pc-bds-exit-reseal-receipt-20260831.json) has
SHA-256 `eb0f208178e00b274d47161c6df3dc63d4f93b9bcf49e7b8d8b09842e4b25d6f`.

The rebuilt development FD has SHA-256
`55a0aa3bff005ab6afa4b39ffbfaf239ffe12084aebd40f1202c87d1593fd8d3`;
the ESP image remained
`a49be97db44c0d68b3382f3b1e46eba2fc7a3b12bcba14c1ec720f0511b71979`.
GitHub-hosted CI run `33460676052` and Security and quality run `33460677655`
both passed at that exact tested head.

Exact-head Studio t14 job `20260831-215843-85527-335` separately completed one
sealed diagnostic in which BDS loaded Windows `BOOTAA64.EFI`, `StartImage()`
advanced without returning before the terminal boundary, and the guest result
record was overwritten after that handoff. Its receipt deliberately records
`single_run` and `windows_kernel_entry_not_proven`, while the diagnostic log
records `windows_boot_proven=false`. This is live single-run signal, not a
fixed-count Windows boot proof.

## Result

Studio tier `t13-bridgevm-pc-bds-exit` job
`20260831-022309-85406-30344` ran on exact tested code head
`f6fbec0e5d5644d412614b1ca040449eac37c049` and completed:

| Field | Result |
|---|---:|
| required lanes | 20 |
| attempted lanes | 20 |
| passing lanes | 20 |
| failed lanes | 0 |
| separate BridgeVM process boots | 20 |
| complete architecture-protocol masks | 20 (`0xfff`) |
| Simple File System handles | 20 |
| loaded removable-media applications | 20 |
| post-`ExitBootServices` results | 20 |
| `ExitBootServices` attempts | 1 in every lane |
| MMIO exits | 28,620 |
| virtual-timer exits | 0 |
| receipt `pass` | `true` |
| host | `Mac17,9`, macOS 26.5 |

Every lane used its own APFS-cloned 64 MiB GPT/FAT16 boot disk and its own
APFS-cloned 64 KiB variables file. Independent inspection found 20 distinct
disk inodes and 20 distinct variables-file inodes. Every disk remained at the
sealed SHA-256
`a49be97db44c0d68b3382f3b1e46eba2fc7a3b12bcba14c1ec720f0511b71979`;
all 20 persisted variables files had final SHA-256
`764d061141bab2327a45da6b1a7f265ebc9dcd4d7043446cbbde86d8dbfc2712`.
Their path-bound aggregate is the receipt's `vars_hash`,
`7200d2e84591a008c303027f02c60bfb5ed2a84b29285b009740694f27049faa`.

All 20 retained lane logs contain exactly the same bounded result:

- post-boot-services stage `11`, architecture mask `0xfff`, one filesystem;
- loaded image `0x11fe20000+0x3000` inside guest RAM;
- 1,872-byte memory map with 48-byte version-1 descriptors;
- one successful `ExitBootServices` attempt;
- 1,431 MMIO exits and zero virtual-timer exits; and
- zero matches for `Illegal resource`, `context error reported`, `unexpected`,
  `probe failed`, `FAIL:`, out-of-aperture or unhandled-MMIO errors.

The BridgeVM-owned BDS policy first required all 12 PI architecture protocols,
signalled the standard EndOfDxe event, disabled the watchdog and recursively
connected controllers. Generic EDK2 `PciBusDxe` and `NvmExpressDxe` then
published one `EFI_SIMPLE_FILE_SYSTEM_PROTOCOL` from the NVMe-backed ESP. BDS
used boot-policy `LoadImage()` on `\EFI\BOOT\BOOTAA64.EFI`, signalled
ReadyToBoot and called `StartImage()`.

The loaded application obtained the real UEFI memory map, retried only the
standard stale-map-key case with a fixed three-attempt bound, and called
`ExitBootServices()`. It wrote stage `11` only after that service returned
successfully. The host runner independently rejected missing handles, table or
image pointers outside RAM, a malformed memory map, a pre-exit stage, an
incomplete architecture mask or more than three attempts.

## Sealed identities

| Artifact | SHA-256 |
|---|---|
| public receipt | `07da55c02e860d88b0b64736f978ecf58c062b5d35ffdbec86ebee13441e5512` |
| complete retained receipt | `d118cd5f3dd7c9611ac8dc5c52d4a8583c832e3df2935d645aed8ab970669b2d` |
| summary | `8423027d6f6b7f34a43b6732803082d25b965ad806d29d39f329af5b91a183fe` |
| tier run log | `4e8dae67caaa5c7f6516deae4cbbdefd380fef01c98c355ed6651eb543baf339` |
| aggregate of 20 lane logs | `c12d65a9fd36b62bddb9c21c0cf368752e09f709538cf286e8f3ba9f4c3a18f0` |
| ad-hoc-signed host runner | `4baa3f5306e0d0974a13cd5f0f1c0c58b720fa733c1928f3cf251b4d1ab92319` |
| development FD | `9bf4152f31bf304a384341ee8f9fce7f9d2fc890b9302a19935e107596575849` |
| firmware volume | `f3d0ef5f0aefb6d5b187fa3ce581b9219683a12fab8ff14cbfada4dd20188a5b` |
| GPT/FAT16 ESP boot media | `a49be97db44c0d68b3382f3b1e46eba2fc7a3b12bcba14c1ec720f0511b71979` |
| initial variables file | `71189f7fb6aed638640078fba3a35fda6c39c8962e74dcc75935aac948da9063` |
| aggregate final lane variables | `7200d2e84591a008c303027f02c60bfb5ed2a84b29285b009740694f27049faa` |
| BDS driver | `90836edba8c7b580785daba92b843394064186d2aff69813799d2a323942ec10` |
| `BOOTAA64.EFI` application | `93f86906c18acdc8be76466ad5ff63f50358c52a68583cea703c1b97696fff85` |
| boot-module build receipt | `0d67fc3d070d73c1643a29a1a77a5b278ab54bffcf31a34fad8bc2ee5dad7c36` |
| complete firmware build receipt | `7a67132b53b6bdcf7abccc082af37fbd43cc41ec3961f8693e9b14761e4ccef1` |

The reproducible development build pins EDK2 commit
`b03a21a63e3bd001f52c527e5a57feddb53a690b`, GCC 16.1.0 and GNU binutils
2.46.1. Its boot-module receipt records package-tree SHA-256
`3ae38090b14a6eac100f6e1a738b2a53a29ad0a0af14cc77c2338ba31f61319d`.
The retained public receipt is
[`bridgevm-pc-bds-exit-receipt-20260831.json`](bridgevm-pc-bds-exit-receipt-20260831.json).

GitHub-hosted CI run `33364067431` and Security and quality run `33364069295`
both completed successfully at the exact tested code head.

## Earlier results retained

Jobs `20260831-020922-67379-17300` and
`20260831-020922-67385-20026` failed before their tiers ran because the
isolated worker did not contain the submitted commit. They provide no guest
evidence and remain failed records.

Job `20260831-021202-68330-14130` did complete the same live gate 20/20 on
code head `b30d2691991f9da335cf0f48c22fb151c087d590`, but hosted CI run
`33363103935` failed because the new example's tests were absent from the
reachability inventory. That live result is retained but was not promoted.
The exact head above adds the missing inventory entry and makes the worker
fetch or fail closed on an unavailable submitted commit, then repeats the
entire fixed sample rather than relabelling the earlier result.

## Claim boundary

This proves the independent development firmware's BDS invocation, complete
architecture-protocol prerequisite, NVMe-backed ESP discovery, removable-media
PE/COFF load, ReadyToBoot transition, memory-map retrieval and code execution
after a successful `ExitBootServices` at fixed N=20.

It does **not** prove Windows Boot Manager, Windows kernel entry, Windows
installation or boot, GOP, MSI/MSI-X, write durability, production firmware or
production signing. The independent board remains experimental, the QEMU
Compatibility Engine remains available, A9 remains OPEN, and B4 remains
separately PROVEN at fixed N=20 with p95 243 ms.
