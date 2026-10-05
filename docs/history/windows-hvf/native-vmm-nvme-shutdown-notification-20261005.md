# Native NVMe shutdown notification — 2026-10-05

Integrated source checkpoint `29f1d4103c00e5e2326aeacf6732081a31b93d4c`
repairs an ignored guest shutdown request. Writing normal or abrupt shutdown
notification to `CC.SHN` previously stored the field without advancing
`CSTS.SHST` or synchronizing the attached storage backends.

## Reproduced transition and repair

Seven unchanged public-MMIO fixtures on the old implementation give
1 PASS / 6 FAIL. Failures show absent shutdown-complete status and zero backend
synchronization attempts. The no-notification/reserved-encoding control passes.
Baseline raw SHA-256:
`d6b9dd29dfefb00efd4dc17cd3c7c933902bb46c382c5ea4cceaa167f5f468c6`.

`crates/bridgevm-hvf/src/nvme/configuration.rs` extracts the existing enable
and reset method, then processes shutdown through the existing all-namespace
flush. Success reports `SHST=10`; a failed flush keeps `SHST=01` and sets
`CFS=1`. Both attached namespaces are attempted, with no repeated automatic
retry before reset. Shutdown status gates subsequent queue processing,
including preexisting backpressured work restored from a snapshot.

Plain reset/re-enable clears shutdown and fatal status. A combined shutdown
notification and enable-clear write resets first, then processes shutdown with
`RDY=0`. Clearing `SHN` alone does not cancel shutdown. The existing snapshot
format already preserves the required controller and queue state.

[NVMe 1.4](https://nvmexpress.org/wp-content/uploads/NVM-Express-1_4-2019.06.10-Ratified.pdf)
sections 3.1.5, 3.1.6 and 7.6.2 define notification, completion and reset;
section 10.5 describes fatal errors that cannot be reported through a completion
queue. The exact failure encoding, shutdown latch and combined-write ordering
are conservative model policies, not uniquely mandated behavior. Their
guest-visible differences from QEMU v9.2.0 are recorded in the
[device-recovery deviation module](../../machine-contract/qemu-virt-deviations-device-recovery.json).

## Paired checks and contract registration

Fixture SHA-256 is identical before and after the repair:
`27d12616294cffea2b3bfc7d427734b93029d19ca623e991fcd1f344132de477`.
Controls cover acknowledged writes, normal queue deletion, abrupt shutdown
with existing queues/backlog, both namespaces and either sync failure,
non-repeated flush, reset/Identify recovery and snapshot preservation. They do
not use commands newly submitted after shutdown as a conformance oracle.

The complete NVMe filter passes 104 tests in debug and 104 in release, with
one preexisting benchmark ignored in each profile. Debug raw SHA-256:
`4ba5adde6cbd74432ecff5af962767df4f595073815b07507456556c2c3f5b9a`;
release raw SHA-256:
`2143778e58d43ae7df89dfd7df6b8b551c2f63c4a1764bdb21c5ca4de09ab811`.
Focused Clippy, formatting, budgets and whitespace checks pass. Independent
source and retained-evidence review found no material blocker.

The registry's flat-module extraction preserves all eight existing objects.
One shared validator checks local and hosted contract inputs; eight module
tests and four freshness tests pass. A metadata-only refinement restricted
module names to the prefix covered by freshness checks, then reran the affected
checks. The already-tested Rust and fixture bytes remained unchanged. Existing
structural ceilings only decrease, and new files use their actual sizes.

## Evidence boundary

Synthetic raw-file sync accounting and injected failures prove the model path,
not physical durability. Memory and read-only overlays remain volatile. No VM,
Windows shutdown, natural `SYSTEM_OFF`, D11 cause or live criterion is proved.
The combined successor still requires complete local project and exact-SHA
GitHub-hosted checks. Criterion states, thresholds, known defects and product
wording remain unchanged; no release promotion is claimed.
