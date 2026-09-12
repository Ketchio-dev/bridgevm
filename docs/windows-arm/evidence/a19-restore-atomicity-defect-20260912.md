# A19 reopened: failed restore leaves a mixed disk/vars pair

## Observed counterexample

On 2026-09-12, the current `snapshot_pair_cli` was built with Rust `1.97.0`
and `--release --locked` at commit `455deb971d8c8ce1b5fdefc14f5ffa134dc4586f`.
The experiment used only temporary eight-byte files, not Windows media.

1. Create a valid snapshot containing `new-disk` and `new-vars`.
2. Create a separate live pair containing `old-disk` and `old-vars`.
3. Set the user immutable flag on the live vars file with `chflags uchg`.
4. Invoke restore, then read both live files without attempting recovery.

The actual result was:

```text
Operation not permitted (os error 1)
restore_exit=1
live_disk=new-disk
live_vars=old-vars
```

The immutable flag was removed and the temporary fixture deleted afterward.
The transcript is retained privately as
`/tmp/bridgevm-snapshot-mixed-pair-reproduction.log`.

## Why the previous claim is wrong

`restore_snapshot` in `crates/bridgevm-hvf/src/snapshot_pair.rs` stages both
files but publishes them using two separate renames. Staging does not make
those publications atomic. Failure of the second rename leaves the first
replacement installed, with neither rollback nor recovery performed.
The comments claiming that every failure leaves a complete pair are wrong.

The macOS product's `HvfWindowsSnapshotCommand` invokes this helper, so this
is not merely an unused experiment. A19's previous `PROVEN` classification
is retracted to `OPEN`; its atomicity requirement is unchanged.

## What remains valid and what remains open

The [sealed normal-path live run](a19-restore-boot-20260912.md) still proves its
single restore-boot marker sequence. It did not inject publication failures
and does not prove interrupted restore safety. Neither that passing run nor
the local project tests override this concrete counterexample.

A fix needs joint publication or an explicitly enforced recovery boundary
that prevents a mixed pair from being used, with failure/crash regressions
and applicable live evidence. Merely checking writable permissions earlier,
retrying a rename, or weakening the acceptance criterion is not sufficient.
