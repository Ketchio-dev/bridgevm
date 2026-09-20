# A19 live selected-generation export contract — 2026-09-20

Classification: historical deterministic evidence. Current product state and
capability wording remain in the
[registry](../../../capabilities/windows-hvf.json). A19 stays OPEN.

## Live-tier path

Source `da7623b76cc57589155aa2b10d0c4a8dd14c2d97` extends the packaged
T20 native snapshot/restore pilot. After the pilot restores the managed
generation, it invokes the packaged
`bridgevm app snapshot-export ID OUTPUT` command and makes the exported
`disk.raw` plus `vars.fd` pair the inputs to the final boot. The final guest
marker check therefore measures the selected exported generation instead of
booting the managed bundle again.

The export verifier requires the exact VM identity and app JSON schema, an
exact two-file manifest, regular non-symlink files, declared byte sizes and
matching SHA-256 values. It rehashes both files after verification and before
boot selection. Any extra file, mutation, identity mismatch, malformed result
or missing retained hash fails closed.

The export directory lives below the pilot's private `$WORK` tree. Cleanup
removes the exported guest disk and variables; the public receipt retains only
the result, manifest, disk and variables SHA-256 values. Disk images, UEFI
variables and other private guest material remain outside git and published
artifacts.

## Deterministic results

- five focused export-evidence and shell-wiring contracts passed, including a
  fake packaged CLI that proves the exact argument order and final boot-input
  switch;
- the five existing native snapshot/restore tier contracts passed;
- negative contracts reject VM identity mismatch, manifest hash mismatch,
  disk mutation, an extra file, symlinked variables, and absent or incorrect
  retained receipt hashes;
- shell syntax, Python compilation, receipt redaction and structural budgets
  passed without raising an existing ceiling;
- the complete source-head project check passed every executable, app,
  security, documentation and structural step except the intentionally stale
  capability identity that preceded this record. Its retained 7,645-line log
  SHA-256 is
  `d23d406652fa37084186b038d847ccffc289e6f5892d22780f632349fb74273b`.

## Evidence limit

No physical-Mac pilot has run on this source. These deterministic contracts do
not prove that Windows boots the exported pair, that the original marker is
present, that the clobbered marker is absent, or that cleanup succeeds after a
real run. They also do not prove physical power-loss behavior, every
filesystem interruption point, or the remaining product-lifecycle sample
count. A19 therefore remains OPEN, and every pass, criterion and capability
promotion flag stays false until the required live evidence exists. Exact
metadata-head local and hosted verification remain required.
