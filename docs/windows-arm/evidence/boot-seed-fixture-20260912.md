# Explicit boot-seed live fixture

Source: `1d29dc363585f89518f8d6e45ff91c3a508ef2e9`.

The optional test `HvfWindowsBootSeedTests.testSeedRealInstalledDiskWhenStaged`
previously discovered fixed temporary paths and deleted a fixed output before
copying a UEFI vars template. It now requires explicit per-run paths:

- `BRIDGEVM_BOOT_SEED_LIVE_DISK`: absolute path to the lane's installed disk.
- `BRIDGEVM_BOOT_SEED_LIVE_TEMPLATE`: absolute path to the vars template.
- `BRIDGEVM_BOOT_SEED_LIVE_OUTPUT`: absolute path to a fresh output vars file.

With no variables supplied the test skips. Partial or invalid configuration
fails rather than silently skipping. Inputs must be readable regular files;
existing outputs and symlink outputs are refused, never deleted. The output
parent must already exist. Each live lane must have its own cloned disk and
vars output; canonical guest images remain immutable.

The fixture copies the template and retains the original seed assertions:
Windows Boot Manager entry, the disk ESP partition GUID, and replacement of
the bundled sentinel GUID. It does not itself boot Windows. Real private-media
execution belongs in the physical-Mac live workflow, not public hosted assets.

## Deterministic evidence

The selected suite ran 15 tests: 14 passed, one explicit live fixture skipped,
zero failed. `scripts/check-project.sh` passed. New tests use tiny private files
to check separate template copying, unchanged inputs, preservation of existing
and symlink outputs, and refusal of partial or relative configuration.

No real installed disk was seeded in this change and no Windows boot was run.
The skipped test is not a criterion pass. These filesystem checks do not claim
hostile concurrent-path substitution safety or a shipping Secure Boot result.
