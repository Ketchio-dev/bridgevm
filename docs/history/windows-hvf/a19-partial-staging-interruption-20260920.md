# A19 partial-staging process exits — 2026-09-20

Classification: historical deterministic evidence. Current product state and
capability wording remain in the
[registry](../../../capabilities/windows-hvf.json). A19 stays OPEN.

## Exercised states

Source `b946e7fe6d59a835cad7c68aec2f714ce07b96fe` extends the
managed-pair restore subprocess contract to three states before atomic
publication:

1. the staged disk clone is complete and file-synced, but vars and the manifest
   are absent;
2. disk and vars are complete and file-synced, but the manifest is absent; and
3. disk, vars and manifest are each complete and file-synced, but their staging
   directory has not been synced.

For each state the child owns the real logical disk-plus-vars lease, creates the
actual managed root and staging directory, and exits through
`std::process::exit` without Rust unwinding. A fresh owner must continue to
select the old complete disk and old complete vars. It then retries the same
restore and must select the new complete pair.

The child entry point is compiled only in tests. Production builds gain no
fault-injection environment switch.

## Deterministic result

The focused contract passed all three fixed child exit codes and both pair
assertions for every state. The complete `bridgevm-hvf` suite passed 996
tests with one existing intentional microbenchmark ignore. Structural budgets
passed with the new test module registered at its actual 112-line ceiling.

## Exact-main physical-Mac result

Job `t20-58275ad1-native-snapshot-export-r5` ran the sealed packaged app from
exact main `58275ad16209051749f67c120af1df4464831b3d`. It completed three of
three Windows boots and three natural shutdowns with 3D injection disabled.
The app CLI created and restored the managed snapshot, exported the selected
restored disk-plus-vars generation, and the final phase booted that exported
pair. The original guest marker returned, the clobber marker was absent, and
worker cleanup was verified.

The public receipt passed the strict verifier. Its SHA-256 is
`74fcf49fbf10f98b2a5371b51b8dcf738c33e094e3f23d01163495353078c3f5`;
the retained run-log SHA-256 is
`5ea183b0d0889e9630c8cdc837037f9ed202e5975e163ccc44c01987caab6100`.
The receipt deliberately retains `claim_eligible`, `criterion_pass`, and
`capability_promotion` as false.

## Evidence limit

This proves process-death recovery from the three durable partial-staging
states. Together with the earlier publication-boundary contract, deterministic
coverage now spans incomplete staging, complete synced staging before
publication, and both sides of atomic publication.

It does not simulate sudden host power loss, prove storage-hardware flush
semantics, or close the remaining product-lifecycle sample count. The one
exact-main T20 run is a live sample, not proof of every interruption state.
A19 remains OPEN. Exact metadata-head local and hosted verification are still
required.
