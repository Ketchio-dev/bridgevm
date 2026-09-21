# A19 snapshot-create process exits — 2026-09-21

Classification: historical deterministic evidence. Current product state and
capability wording remain in the
[registry](../../../capabilities/windows-hvf.json). A19 stays OPEN.

## Exercised create boundaries

Source `bf15b7590f42dfc34b58d6006bff3f2617f8a209` runs the real powered-off
snapshot-create pipeline in child processes and terminates without Rust
unwinding at five durable boundaries:

1. after the staged disk clone or copy is file-synced;
2. after staged disk and variables are both file-synced;
3. after the manifest is atomically written, file-synced and renamed;
4. after the complete staging directory is explicitly synced; and
5. after atomic snapshot publication and parent-directory sync.

Every scenario begins with a complete old snapshot. A fresh parent verifies
that the old snapshot remains selected at the first four boundaries and that
the complete new snapshot is selected after publication. It then retries
snapshot creation and verifies the replacement disk, variables and manifest as
one complete pair.

The observer is crate-internal and receives no repository, process environment,
command-line or user input. Production calls instantiate it as a no-op; the
subprocess exit behavior exists only in the test target.

## Deterministic result

The focused contract passed all five fixed child exit codes and every
post-crash selection and retry assertion. The complete `bridgevm-hvf` library
suite passed 999 tests with one existing intentional microbenchmark ignore.
Structural budgets passed: the original snapshot-create module was reduced
from an 82-line ceiling to 77, and the three extracted modules were registered
at their actual 27, 8 and 87-line sizes.

## Evidence limit

This closes deterministic process-death coverage at the declared durable
boundaries of snapshot creation and complements the retained restore-side
staging and publication contracts. It does not simulate termination during an
individual file copy, sudden host power loss, storage-hardware flush behavior,
a real Windows boot or another product-lifecycle sample. A19 remains OPEN, and
the product remains Engineering Preview with 3D outside the release path.
Exact metadata-head local and hosted verification remain required.
