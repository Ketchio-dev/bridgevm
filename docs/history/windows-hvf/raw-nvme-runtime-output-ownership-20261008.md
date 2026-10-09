# Raw NVMe runtime output ownership — 2026-10-08

Evidence: deterministic synthetic ownership/export tests and static review.
No VM, Windows workload, private media, live gate or power-loss proof.

The actual native stop helper bypassed RuntimeLease for raw-file snapshots.
Root acquired runtime B over a 512-byte disk/vars pair, then runtime A over a
separate raw source with B's disk as its snapshot destination. Both acquisitions
succeeded. Raw export reported success and replaced B's disk; B's vars stayed
unchanged. The corresponding memory path returned WouldBlock and preserved both.
The first test invocation selected zero tests; the corrected invocation ran two,
with one actual raw failure and the passing memory control.

Raw snapshots now stream through the captured slot policy and existing owner.
Destination ownership precedes staging, copying and publication. The shared
writer keeps exclusive staging, file sync, rename, parent sync and gap-free
lease rebinding. Producer callbacks cannot choose another output pathname.
Absent captured snapshots invoke no producer; later external configuration
changes cannot add, remove or redirect that captured snapshot. Both namespaces
use the same bounded 1 MiB overlay-aware stream, including a final partial chunk.
Write-back still flushes the already-open backend after snapshot publication.

Review exposed a first-repair lifetime gap: snapshot-to-self replaced the input
pathname and released its old inode despite the raw backend retaining that file.
Actual helper and open-file regressions reproduced premature alias acquisition,
including after vars persistence. MediaLease now supports explicit lifetime pins;
raw export pins each captured input once, before publication. Pins survive
retries and uncertain errors without accumulating superseded export generations.
Ordinary memory persistence still releases old unpinned vars/output inodes.

Reused runtime staging also had a case-alias defect: an isolated child on the
case-insensitive test volume reported success then lost its uppercase output.
ASCII case-insensitive staging-basename exclusion repairs this reproduced case;
cleanup still removes only its own surviving inode. No general namespace-race
protection is claimed.

Root validation: media14/0 plus existing ignore, runtime38/0, actual helper8/0,
restored HVF library1378/0 plus existing ignore and probe example527/0. All-target
Venus Clippy and formatting pass. Removing destination admission reproduces
changed foreign disk bytes even though later rebinding returns an error; removed.
Publication-error tests retain uncertain output ownership without issuing success.
Short-write, overlay/tail, reader/writer failure and pin-acquisition tests pass.
A test type-inference failure and parallel timestamp fixture collision are
retained; unique create-dir retry fixes the fixture without changing assertions.

This is cooperative ownership, not protection from arbitrary host writers.
Earlier writes are not rolled back on later failure. Exact sealed full and
hosted checks remain required; product criteria and release state are unchanged.
