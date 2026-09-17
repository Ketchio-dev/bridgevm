# Native app snapshot restore live tier — 2026-09-17

## Result

Source `0463bfb625e5f6166a51e0198152ffae4af3c307` adds the physical-Mac tier
`t20-a19-native-snapshot-restore`. It turns the native app snapshot CLI into a
live, guest-visible restore experiment without changing A19's acceptance
criteria. No physical-Mac job has run at this checkpoint, so A19 remains open.

## Sealed path

The submission manifest authenticates the exact release app tree, its paired
`bridgevm` CLI, `BridgeVMControl` executable, bundled `snapshot_pair_cli`,
release `hvf_gic_boot_probe`, installed Windows disk, and matching UEFI variable
store. The worker reauthenticates those inputs before and after the experiment.
Disk and variables are APFS-cloned into a new private native library; the source
pair is never launched or modified.

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
`claim_eligible`, `criterion_pass`, `capability_promotion`, and
`three_d_injection` to false. One pilot therefore cannot promote A19.

## Deterministic evidence

The new contract suite passed five tests covering input relationships, source
mutation refusal, APFS-clone identity, strict receipt accounting, public
redaction, queue sealing, paired CLI routing, and the 3D-off registration. The
retained snapshot seal, cleanup, and clone-permission suites passed 5, 3, and 2
tests. The receipt redactor passed 27 checks, and the Windows product E2E
contract suite also passed. Shell syntax, formatting, structural budgets, and
diff checks passed.

These results prove deterministic wiring and fail-closed accounting only. They
do not prove a Windows boot after restore, interrupted-operation safety, raw
export, application consistency, a live sample, or A19 completion. A live pilot
may be submitted only after this exact source has passed local and hosted
verification and the packaged exact-main artifacts have been sealed.
