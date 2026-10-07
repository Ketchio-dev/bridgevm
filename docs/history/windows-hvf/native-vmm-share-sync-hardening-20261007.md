# Native VMM shared-folder sync hardening — 2026-10-07

Integration starts from main `158f5cca`.

## Defects

Shared-folder sync polls guest-controlled `LSR` listings, fetches files with
`GET`, and writes/deletes below the configured host share. Review reproduced:

- Oversized-file skip memory retained every historical `(name, mtime)` pair.
  Twenty listings of 1000 fresh oversized names retained 20,000 entries.
- Lexically valid names could traverse pre-existing host symlinks during
  `create_dir_all`, `fs::write` or parent resolution for `remove_file`. Host
  scanning skipped symlinks, but that did not protect guest-initiated writes.
- Failed writes were recorded as synchronized. A missing destination then
  caused guest deletion; an old or partially written destination could instead
  be uploaded over the intact guest version.

The initial unmerged repair marked failed hashes unknown and suppressed
same-version retries. Review disproved its sufficiency: three new fixtures
failed on stale-host upload, same-version retry and guest-file resurrection.
That experiment is retained as a failure, not final validation.

## Repair and boundaries

Skip memory now retains only currently listed oversized versions. Failed
writes have explicit state, suppress reverse upload and both deletion paths,
and retry on the normal guest-listing cadence. A retry stays pending until
filesystem publication succeeds. Forgetting a failed name requires the guest
to unlist it and a targeted filesystem check to confirm host absence; an
omitted scan entry or access error is not absence. At 4096 failed records or
4 MiB of failed-name bytes, the session disables sharing and drops queued
share operations rather than evicting data-loss protection. Other console
service requests remain available; there is no automatic unsafe restart.

Guest writes pin the selected root and walk/create child directories with
no-follow directory descriptors. The selected root itself may be a symlink.
Writes use an exclusive temporary file in the verified parent, complete and
sync its bytes, then publish with `renameat`. Errors preserve the previous
file. Deletes use no-follow metadata checks and `unlinkat`. Swapping a child
symlink cannot redirect these operations to its target; this is not a claim
of confinement against a hostile host moving the pinned directory elsewhere.

Existing regular-file write access is required. Mode is retained; on macOS,
ACL/xattr copy failure refuses publication rather than silently dropping
metadata. Such a refusal can be stricter than the old direct-write behavior.
Replacement creates a new inode, so hardlink identity is not preserved.
New files are private mode 600. Optional timestamp availability does not turn
a successful publication into a failed transfer. No power-loss claim is made.

## Deterministic evidence

- Failure-state tests cover old/partial destination suppression, unchanged
  guest-version retry, unlisting and temporary host-scan omission.
- Count/byte-budget tests cover refused-name churn and non-inflating retries;
  a console fixture confirms queued PUT/DEL removal while retaining PING.
- Real filesystem tests cover atomic failed-write preservation, parent/leaf
  symlink substitution, root-link acceptance, mode preservation, read-only
  refusal and unreadable-file deletion. A temporary-name/leaf collision found
  in review is covered so successful publication cannot delete itself.
- Example tests pass 509/0 with and without Venus; clippy and formatting pass.
  Full-project and exact-SHA hosted validation remain pending at this record.

Helpers and tests are split into dedicated modules; existing structural
ceilings are not raised. A11 remains OPEN. Capability states, thresholds,
known defects and product wording are unchanged. No live Windows or release
claim is made from these deterministic results.
