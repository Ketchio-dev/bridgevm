# BridgeVM Virtual ARM PC: UEFI variable restore across VMs and processes (2026-08-30)

## Evidence rank and result

This is a fixed-`N=20` live engineering receipt on real hardware, not a product
release criterion. At exact BridgeVM code head
`cf8ad54f0f99f87e574d6af152ea8b64fe3d6d6f`, all 20 independent host
processes completed the same two-boot sequence on a `Mac17,9` running macOS
26.5:

1. initialise a private 64 KiB variables backing to erased bytes;
2. create an HVF VM with fresh RAM, enter BridgeVM firmware, and write one
   non-volatile UEFI variable through `SetVariable`;
3. destroy that vCPU and VM completely;
4. create a second HVF VM with a separate fresh RAM allocation but the same
   variables backing; and
5. retrieve the exact payload through `GetVariable` and validate the service
   pointers, attributes, quota, variable-store header, ACPI and SMBIOS tables.

The result was `20/20`; every lane reached `dxe_result=11` and
`variable_state=2`. A representative final result was:

```text
hvc_iss=0x0 args=[100002000, a, 3c4]
hvc_iss=0x0 args=[100002000, b, 3c4]
BridgeVM Virtual ARM PC variable restore probe: PASS
board=com.ketchio.bridgevm.virtual-arm-pc abi=1
reset_vector=0x0 firmware_size=0x4000000 ram=0x100000000
sec_result=1 hob_count=8 hob_list_gpa=0x100004000 hob_list_size=320 dxe_result=11 system_table=0x11ffb0018 runtime_services=0x11ffbff18 runtime_protocol=0x11ff82000 runtime_crc32=0x3f6f728d variable_state=2 variable_attributes=0x7 get_variable=0x11ff73dc8 set_variable=0x11ff75cc4 query_variable_info=0x11ff736a4 variable_max_storage=65508 variable_remaining_storage=65400 variable_max_size=964 configuration_entries=7 acpi=0x26001000 smbios=0x2600c000 result_gpa=0x100001000 boot_info=0x26000000
firmware_sha256=37c659e4ec70050790607ab58ec8eb9066284f13eedccb50795cf4623c642172
vars_initial_sha256=71189f7fb6aed638640078fba3a35fda6c39c8962e74dcc75935aac948da9063
vars_written_sha256=c8589a00b6ca778e8333a3fed839ad2bd780854264823f69df5435d7ef5226f6
vars_restored_sha256=c8589a00b6ca778e8333a3fed839ad2bd780854264823f69df5435d7ef5226f6
LIVE PROOF: a recreated HVF VM restored the non-volatile UEFI variable from the preserved vars backing
```

The complete exact-head 20-lane log has SHA-256
`01f264b9e6a5f72b924f90f5d10b646aa34ed2743e67422567f08d138f11c156`.
The local result record has SHA-256
`5503b0c9c493c7451cdfb6d6ea8f919fe2d2132e110cd7c9ee544217b16d3c4d`.
The ad-hoc-signed runner carries only the Hypervisor.framework entitlement and
is a local probe, not a redistributable product binary.

## Separate-process follow-up

At exact host-runner code head
`7e749f814ef5f422eba8d57d6cc69e32103a9148`, a second fixed-`N=20`
campaign moved the preserved backing across a real process boundary. Each lane
owned a distinct 64 KiB file and launched the signed runner twice:

1. the first process required an absent target, started from erased bytes,
   reached `dxe_result=10` / `variable_state=1`, synchronised a mode-0600
   temporary file, and published it without replacing an existing path;
2. that process exited completely;
3. a second process opened only an exact-size regular file without following a
   final-component symbolic link, created a new HVF VM and fresh RAM, and
   reached `dxe_result=11` / `variable_state=2`; and
4. an additional write-mode invocation had to reject the now-existing target.

All 20 lanes passed both processes and the no-overwrite check. Every written,
loaded, final and on-disk payload had SHA-256
`c8589a00b6ca778e8333a3fed839ad2bd780854264823f69df5435d7ef5226f6`.
A representative lane reported:

```text
BridgeVM Virtual ARM PC variable-file stage: PASS
sec_result=1 hob_count=8 hob_list_gpa=0x100004000 hob_list_size=320 dxe_result=10 system_table=0x11ffb0018 runtime_services=0x11ffbff18 runtime_protocol=0x11ff82000 runtime_crc32=0x3f6f728d variable_state=1 variable_attributes=0x7 get_variable=0x11ff73dc8 set_variable=0x11ff75cc4 query_variable_info=0x11ff736a4 variable_max_storage=65508 variable_remaining_storage=65400 variable_max_size=964 configuration_entries=7 acpi=0x26001000 smbios=0x2600c000
process_mode=written
vars_loaded_sha256=71189f7fb6aed638640078fba3a35fda6c39c8962e74dcc75935aac948da9063
vars_final_sha256=c8589a00b6ca778e8333a3fed839ad2bd780854264823f69df5435d7ef5226f6
vars_file_sha256=c8589a00b6ca778e8333a3fed839ad2bd780854264823f69df5435d7ef5226f6
BridgeVM Virtual ARM PC variable-file stage: PASS
sec_result=1 hob_count=8 hob_list_gpa=0x100004000 hob_list_size=320 dxe_result=11 system_table=0x11ffb0018 runtime_services=0x11ffbff18 runtime_protocol=0x11ff82000 runtime_crc32=0x3f6f728d variable_state=2 variable_attributes=0x7 get_variable=0x11ff73dc8 set_variable=0x11ff75cc4 query_variable_info=0x11ff736a4 variable_max_storage=65508 variable_remaining_storage=65400 variable_max_size=964 configuration_entries=7 acpi=0x26001000 smbios=0x2600c000
process_mode=restored
vars_loaded_sha256=c8589a00b6ca778e8333a3fed839ad2bd780854264823f69df5435d7ef5226f6
vars_final_sha256=c8589a00b6ca778e8333a3fed839ad2bd780854264823f69df5435d7ef5226f6
vars_file_sha256=c8589a00b6ca778e8333a3fed839ad2bd780854264823f69df5435d7ef5226f6
PASS: separate BridgeVM processes restored one fail-closed vars file
```

The immutable local seal is
`bridgevm-pc-variable-process-7e749f814ef5`. Its exact identities are:

| Item | SHA-256 |
|---|---|
| complete 20-lane log | `1aaa83d170f3f958e0e7374da9705ad026228d1eb0bad3d28be9b263fd3683cb` |
| result record | `7acf26c25b8466bbb67aac38b151b6a3277b8de5a649a9f8fefd1eb6845866e1` |
| 27-file content manifest | `6b3cca9fea42c3bd379c563a8681743f8160692589633b354b2dae8e4403900f` |
| signed exact-head runner | `dcd815088fe4f390e105958a91767283e42485a208aa3fb56ab82c932cbb3480` |
| exact Git source archive | `0ac2912f7739559d89001d54ad85368a09d2e8d57b0c1e49c4e53dcfba76a3fc` |

The exact-head firmware rebuild retained FD SHA-256
`37c659e4ec70050790607ab58ec8eb9066284f13eedccb50795cf4623c642172`,
FV SHA-256
`482e0803bbeb009e49b4bac4f24c0011734604cb1b4e7c67f705f917d69262da`
and build-receipt SHA-256
`0ea74bd7fe55320f8d1f77c6b77a42a71c5c27476a12157c5b26f490ac3e55a9`.
The seal was made read-only after its content manifest was computed.

## What changed

The development firmware volume now contains pinned generic `RuntimeDxe` and
`VariableRuntimeDxe`, followed by BridgeVM's platform-table and variable probe
drivers. The variable module uses the first 64 KiB of the board's reserved
variables aperture. The probe requires the standard Runtime, Variable and
Variable Write Architectural Protocols before it can dispatch. It writes the
result stage only after all service calls and returned data have passed.

The reset continuation establishes a bounded Armv8-A identity map before DXE,
and its HOB list reserves the three 4 KiB page-table pages so DXE cannot reuse
them. The DXE Core and runtime-service build descriptions link
`ArmCacheMaintenanceLib` and fail if the resolved DXE Core library list falls
back to `BaseCacheMaintenanceLib`. Runtime and variable builds also require
`BaseDebugLibNull`, preventing an accidental dependency on console-backed
debug output.

The host validator does not trust the firmware stage alone. It checks the
eight exact HOBs, bounds every service pointer to mapped guest RAM, matches
`GetVariable`, `SetVariable` and `QueryVariableInfo` to the EFI runtime table,
requires attributes `NV | BS | RT`, checks returned quota bounds, validates
the authenticated variable-store header, and requires the backing hash to
change after boot one and remain identical after boot two.

The separate-process runner uses the same validator before publishing or
accepting a backing. New files are written to a same-directory temporary file,
synced, hard-linked into an absent destination, unlinked from the temporary
name, and followed by a directory sync. Restore mode uses `O_NOFOLLOW` and
requires an opened regular file of exactly 64 KiB. A file that already exists,
a symbolic link, a short file or a failed sync is an error rather than a reason
to continue with erased or partially trusted state.

## Failed candidates retained

The intermittent failure was not rewritten as success. The aggregate candidate
logs are retained by hash. Their durable local manifest has SHA-256
`90f2eb533b31ce6d7d2e06c88eafd40f1edaff1214e759eb0c7124b00f0a4782`:

| Candidate | Result | Aggregate log SHA-256 |
|---|---:|---|
| initial MMU/variable path | 8/20 | `58e33614890a14c27ec771dd736e6b00d851bd4de7e5a923a434fe9550f54ad4` |
| console-independent DebugLib | 14/20 | `b2cd177096394aad8f364bf265ae5072f4e6dbf74bc0ac61b2a887e771ab6060` |
| blanket `ic iallu` at reset | 7/20 | `e84146af837487e3c53643b4e45be93304758bf2e7568f3adcc141d108416837` |
| fresh RAM per recreated VM | 12/20 | `98a809ee921a025fb55c96392db046ad812679ca5587e7103ada7476a0f3736d` |
| page-table HOB with a stale host parser | apparent 0/20, invalid measurement | `6941fcb713d7b0884bad03fa363eaa35de23577948a00702205b60968e98788f` |
| page-table HOB with corrected parser | 10/20 | `99e9748ac1b3d014bfd836d355b226b52bf839c762cfc7b22a5bba9f869cff54` |
| BTI hypothesis | 12/20 | `2fa08fa7b181ed43074eb13ced0a0e277253c21f17b2795ae9670c61aacfde11` |
| real DXE Core cache maintenance, diagnostic build | 20/20 | `77e43810d693786dcfa361b6dea045b5dbb3f733b1cae9c0f698f4ebd420b0cf` |
| real cache maintenance, diagnostics removed | 20/20 | `c2a2c8431538cc1d932204c48204e886f3108bdbde4338f8e2ee6096a88bc754` |

The failures trapped with `ESR_EL1=0x02000000` at addresses inside loaded DXE
images even though the host copy at the same GPA contained valid AArch64
instructions and the translation descriptors remained intact. Nulling console
debug output, reserving the page tables, recreating RAM and changing BTI state
did not remove the intermittency. The decisive change was replacing the
pinned generic DXE Core's null Arm cache-maintenance resolution with the real
Arm implementation used after PE/COFF image loading. The final clean and
exact-head samples then both reached 20/20. The page-table reservation and
fresh-RAM separation remain because they are independently required ownership
boundaries, not because they are presented as the root cause.

## Reproducible identities

Two consecutive builds from the detached exact-head worktree compared the FD,
FV and JSON receipt byte-for-byte equal.

| Artifact | SHA-256 |
|---|---|
| raw DXE Core | `47890d197075f56b6ede34697426ac56f40672994586f506079db13eb0d29ce7` |
| standalone DXE Core FV | `b8d87876dbb88e232ee0eb418008da43fe41ac8219c4076a38d9dc85fd63a347` |
| fixed-rebased DXE Core | `cfe2ea1a7dc5573b4a5f6952e9177475ef91fb41da40c08e89c6f48bec2a4d90` |
| generic RuntimeDxe | `5b3a37c1403e77b51b8dafb16a796dcbc3080692617ff0eae8b759c979b4260d` |
| generic VariableRuntimeDxe | `3d6f0fbd9d155f76d6f1001ee67fce25e36bd5aef20bea088421a772a7500a90` |
| BridgeVM PlatformTablesDxe | `16b3fdd6ede6d5aea14d26419351cf262ef358692fd28682dbbafe74c22438b5` |
| BridgeVM variable probe | `10e591dbe5d3c93aafe26d882b25fe5682c5aea782f6ce03ee2f26ff0dbc0bbb` |
| reset/SEC/HOB/MMU/IPL vector | `3ec6ddb04175dbfbd84dc784a264c8f487410c03a0c4d4a38f630e8b06032226` |
| combined firmware volume | `482e0803bbeb009e49b4bac4f24c0011734604cb1b4e7c67f705f917d69262da` |
| complete 64 MiB development FD | `37c659e4ec70050790607ab58ec8eb9066284f13eedccb50795cf4623c642172` |
| BridgeVmPcPkg source tree | `632630227397c095c823d657a812cce2abae538b8a54c0907247c62888f94497` |
| JSON build receipt | `0ea74bd7fe55320f8d1f77c6b77a42a71c5c27476a12157c5b26f490ac3e55a9` |
| signed exact-head probe runner | `8822c2bd9004af338645189fe0e430199d046dc005558f3c57916c9fb04cdf07` |

The earlier reset/SEC/HOB-only FD remained byte-identical at
`8db976249ff86c9613d0a13a0d811ee68a94ef90835d524deb451b927bc332d6`
and its bounded HVF regression passed after this integration.

## Claim boundary

This proves that the development firmware can write and read one standard UEFI
non-volatile variable, first across a recreated HVF VM with fresh RAM and then
across two separate BridgeVM processes using one explicitly preserved file per
lane. It also retains the earlier Runtime Architectural Protocol, CRC32, ACPI
and SMBIOS results.

It does **not** prove persistence across a host reboot or power loss; recovery
from a process crash during publication; authenticated variable policy; Secure
Boot; `SetVirtualAddressMap`; time or reset services; `ExitBootServices`; BDS;
GOP; block I/O; Windows installation; or Windows boot. The independent board is
still experimental and not used by the shipping Windows path. A9 remains OPEN.
The separate B4 product result remains PROVEN at 20/20 with p95 243 ms.
