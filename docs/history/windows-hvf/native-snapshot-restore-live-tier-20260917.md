# Native app snapshot restore live tier — 2026-09-17

## Result

Pilot `t20-e6481e5b-native-snapshot-r1` failed before VM execution. The queue
had copied the Venus probe Mach-O without its app-relative Frameworks, so dyld
could not load `libvirglrenderer.1.dylib`. Source
`f9a035730dcdaccfa3aa9baa92479d3724e5bdcf` retains that failure and seals the
whole authenticated app runtime for the next pilot. A19 remains open.

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
`claim_eligible`, `criterion_pass`, `capability_promotion`, and
`three_d_injection` to false. One pilot therefore cannot promote A19.

## Deterministic evidence

The contract suite passed five tests covering fixed app relationships, staged
mutation refusal, clone identity, strict receipt accounting, redaction, queue
sealing, paired CLI routing, and 3D-off registration. The exact-main app clone's
probe passed dyld loading from its staged relative Frameworks. The 44-check
Windows product live-tier contract, syntax and structural budgets passed.

These results prove deterministic wiring and fail-closed accounting only. They
do not prove a Windows boot after restore, interrupted-operation safety, raw
export, application consistency, a live sample, or A19 completion. A live pilot
may be submitted only after this exact source has passed local and hosted
verification and the packaged exact-main artifacts have been sealed.
