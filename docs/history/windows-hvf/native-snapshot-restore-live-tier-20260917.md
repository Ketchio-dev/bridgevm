# Native app snapshot restore live tier — 2026-09-17

## Result

The first exact-main pilot failed before VM execution because its Venus probe
lacked app-relative Frameworks. The second stopped at an expired-password
screen. Both remain retained. Exact-main job
`t20-b9fe29fa-native-snapshot-r3` passed with the canonical agent pair: three
Windows boots, three natural shutdowns, and original/clobbered/restored marker
proof. Receipt SHA-256 is
`3326e5a7d19219aefd43e446aa5e6c9cfebf6d24b001447f9b2ca1b6fdc06a3b`.
A19 remains open because interrupted restore and raw export are still unproven.

## Sealed path

The submission manifest authenticates the exact release app tree, its paired
`bridgevm` CLI, `BridgeVMControl` executable, bundled `snapshot_pair_cli`,
release `hvf_gic_boot_probe`, installed Windows disk, and matching UEFI variable
store. The worker APFS-clones the app and executes its CLI and probe from that
sealed tree, preserving relative Frameworks. It reauthenticates source and
staged trees before cleanup. Disk and variables are separately APFS-cloned into
a private native library; the source pair is never launched or modified.

The generated native registration sets `experimental3DAllowed` to false and
turns networking off. The experiment then:

1. boots the cloned pair, writes an original marker to the guest C: drive, and
   observes a natural PSCI shutdown;
2. creates and verifies a powered-off snapshot through
   `bridgevm app snapshot-create`;
3. boots again, reads the original marker, replaces it with a distinct clobber
   marker, and observes another natural shutdown;
4. restores through `bridgevm app snapshot-restore`; and
5. boots the restored pair, requires the original marker and absence of the
   clobber marker, then observes the third natural shutdown.

A passing receipt requires every sealed and produced artifact hash, exactly
three attempted/passed boots, exactly three natural shutdowns, private clone
cleanup, and strict public-redaction revalidation. It fixes
`claim_eligible`, `criterion_pass`, `capability_promotion`, and `three_d_injection`
to false. One pilot therefore cannot promote A19.

## Deterministic evidence

Five contract tests cover app relationships, mutation refusal, clone identity,
receipt accounting, queue sealing, CLI routing, and 3D-off registration. Hosted
CI passed before r3. Source `be3c45d0786d1c1cec4915fa34c2c14de068a920`
makes inline app-JSON validation fail closed against the packaged CLI output.

The r3 receipt proves one packaged native app restore journey at exact main
`b9fe29fae274aceacdd9a7b33df81810ca7fb675`. It does not prove interruption,
raw export, application consistency, or A19 completion.
