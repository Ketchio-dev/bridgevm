# NVMe export temporary ownership — 2026-10-08

Evidence: deterministic tiny-file native tests and static review. No guest media,
VM, live workload or power-loss campaign was used.

The raw-file NVMe exporter used a fixed sibling `.output.raw.export` and opened
it with File::create. Actual runtime media admission accepts a source with that
name and an output named output.raw. Holding its RuntimeLease does not prevent
the exporter itself from truncating the source before reading. Root reproduced
an 8192-byte read-only-backend source becoming zero bytes, then UnexpectedEof.
A second baseline consumed an unrelated existing fixed-name file even while
returning export success. Neither requires concurrency or a malicious guest.

Export now creates a separate, mode0600 temporary inode exclusively. Occupied
names are not adopted or truncated. Bounded candidate retries use PID/counter
only for uniqueness; these are not secrets and create_new is the ownership
boundary. Staging remains owned through copy, sync, rename and parent sync.
Descriptor/path inode identity controls cleanup of remaining staging on errors;
a replaced entry is not removed. Bare relative destinations use parent `.`.

The first repair excluded destination names case-sensitively. Review identified
an absent uppercase destination aliasing the generated temporary name on common
macOS filesystems. Root actually reproduced reported success followed by a
missing destination on case-insensitive APFS: Drop deleted the published inode.
ASCII case-insensitive basename exclusion repairs that error. The failed repair
experiment remains recorded; it is not reclassified as a successful baseline.

Tests cover actual runtime admission plus public controller export, old fixed
name regular/hardlink/symlink preservation, occupied generated file/hardlink/
dangling-symlink collisions, isolated case-alias reproduction, read failure,
rename failure, private mode and identity-safe cleanup. Export/source bytes are
small synthetic fixtures and fixture-owned temporary trees remove themselves.
NVMe190/0 plus one existing ignored microbenchmark; full HVF library1370/0 plus
that ignore; Clippy warnings-denied and rustfmt pass. Static review confirmed the
case correction; reviewer did not independently execute the root tests.

No arbitrary concurrent namespace mutation protection or raw-output lease
redesign is claimed. Destination publication and sync error semantics remain;
this does not prove durability under power loss. New exact sealed local/hosted
checks remain required. No product/criterion promotion, permissions or fence change.
