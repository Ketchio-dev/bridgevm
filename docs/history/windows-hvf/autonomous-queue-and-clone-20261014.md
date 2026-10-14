# Clipboard, full-clone and queue response corrections — 2026-10-14

Evidence rank: **test**. This batch follows the
[selected-media checkpoint](autonomous-selected-media-20261014.md).
Product state, capability wording, thresholds and known defects are unchanged.
A9, A11 and A19 remain OPEN. No Windows guest or physical live gate ran here.

## Host clipboard ownership across queued messages

The host poller could observe a new Mac copy and enqueue its event, then accept
an older guest reply before the service consumed that event. Both orderings
reproduced divergence through the actual service, worker and channel paths.
The original poller and an extraction-only worker each failed the same two cases.
A separate startup/disconnect baseline had two failures and one passing control.

Host events now carry a checked observation revision. Guest writes carry the
last revision consumed by the service; the worker refuses stale revisions.
The worker observes initial host content before writes and discards a batch
whose service is already disconnected when the command drain completes.
Revision exhaustion stops the worker. The guest protocol is unchanged.

The final agent-console selection passed 169 methods, including eight new
controls; the non-test example compiled without warnings. Tests use a memory
pasteboard and modeled guest, never the operator's clipboard. Same-text recopy,
empty/nontext suppression, disconnect after the batch check and the native
changeCount/write race remain outside this repair. Application paste consumption
is not established. An initial formatting failure and its correction are retained.

## Independent QCOW2 backing paths in full clones

The public storage API reported a successful full clone while copied QCOW2
headers still referred to the source bundle. Hiding only the owned synthetic
source made actual qemu-img readback fail; the raw-copy control passed.
The clone now validates the copied backing graph and rebases its headers to
owned relative dependencies before publishing its metadata. Unsupported external
backings fail, and cleanup retains the existing exclusive-destination boundary.
The linked-clone branch and fixed-path release helper policy are unchanged.

Five new graph methods on unchanged production had one pass and four failures;
the repaired methods passed. Final storage validation passed 117 unit methods
and eight literal-import integration methods. Real multi-level/literal backing
readback, source-byte preservation, helper failure, occupied output and file/
directory symlink refusal are covered. A separate raw debug process passed with
an empty executable search path. This is not release-helper execution proof.

Six old copy fixtures used plain bytes labeled QCOW2. Copy-only fixtures now
explicitly use raw format; the snapshot metadata fixture creates a real overlay.
Their assertions remain, and the original failed selection remains recorded.
The first public baseline executable was overwritten at a mutable Cargo output
path before archival. Its old hash is historical only; its failed log, fixtures
and authenticated reconstructed sources remain. A later five-method baseline
binary was archived. This retention correction is part of the bound evidence.
The shared graph helper comment now describes owned validation; full clone does
not acquire the import caller's separate runtime byte-comparison guarantee.

## Navigation and truthful NVMe allocation responses

With a VM selected, the top-level dashboard route ignored the command palette's
Pro-mode choice. A one-line guard now lets the existing detail router display
the fleet table. Explicit import and retained-control selections retain priority.
The same three new methods changed from two passes/one failure to three passes;
17 related native methods and the new three actual-shim methods passed.
These tests inspect composed SwiftUI values and invoke actual palette closures;
they do not establish rendered visibility, physical keyboard or dismissal behavior.

NVMe SET Number of Queues returned a request-limited count while GET CURRENT
reported the fixed eight-queue capacity. Four of six admin SQ/doorbell/CQ tests
failed before repair. SET now reports the existing fixed allocation, consistent
with GET and queue creation; no capacity, reset or snapshot state changes.
[NVMe 1.4 section 5.21.1.7](https://nvmexpress.org/wp-content/uploads/NVM-Express-1_4-2019.06.10-Ratified.pdf#page=212)
permits allocation above the request. No new intentional QEMU deviation is added.
All six controls and 218 related NVMe methods passed; one existing microbenchmark
remains ignored. This is deterministic device evidence, not guest-boot proof.

## Failure diagnostics and dependency maintenance

The staged-read interruption observer previously lost the child's numeric status
when observation failed. A private exclusive failure record now retains the known
Popen returncode, or an explicit unknown value, after existing cleanup. It records
only exception type and bounded status fields. Receipt failure preserves the
original exception; existing files and symlink targets are not overwritten.
The stop-point, signal, wait, cleanup and acceptance logic is unchanged.

The unchanged observer failed seven assertions across six baseline methods.
Final live-tier contracts passed 19 methods, related cases 11, entrypoint two,
and the synthetic auxiliary CLI three. A real already-exited Python fixture
binds status to Popen. No packaged snapshot helper was used; the previous actual
tiny T22 observation remains failed and supplies no interruption/retry acceptance.

The six exact lock updates from Dependabot 321 and 19 action-pin replacements
across nine workflows from Dependabot 389 are integrated. Consumer tests passed
439 methods; Rust 1.85 workspace and Linux stub compilation also passed. Old PR
failures were source-freshness refusals, not observed dependency compile failures.
The committed candidate correctly requires a new registry checkpoint. Action
changes still require exact hosted execution; historical PR checks cannot supply it.

## Integrated checkpoint and retained evidence

Parent 91f734ea passed all 45 local project stages and its 109 originally recorded
hosted runs. Those receipts belong to that parent. This successor still needs its
own full local and exact hosted checks. Independent lane/source reviews found no
blocker. Structural ceilings are not increased; new modules use their actual sizes.
Failures and scoped limitations above remain part of the record.

| Retained artifact | SHA-256 |
| --- | --- |
| Clipboard receipt | `660792b59696b52bedf3b121b328a6f9151b851ff396035b6ae859c02e2461af` |
| Full-clone final receipt | `5f897b203323978e82d0a4c4d11329682e3200cdbdb0449995c1503196dbf1ba` |
| Full-clone independent review | `f0e48cf4f1125064c273c42178dc7c153fb9d0754a8e6dc2686bcbd853a01b51` |
| Navigation receipt | `8a59807494d128d7247cbe6947d88a8176aa22558e479058c66fc608e77ba317` |
| NVMe receipt | `7b7acd796abc4e4012c97f386bd1f6101bfba6c26f5adf5262c1552a64a8e1df` |
| Observer report | `5aa9e29cb3bd4a7822250dac296a452725949450608d5f4beb6d2eb80f0bb533` |
| Dependency receipt | `5f1109ed2b9e975c88106097f22d62f9fe83f2ee39aa690dba6ad7d016922163` |
