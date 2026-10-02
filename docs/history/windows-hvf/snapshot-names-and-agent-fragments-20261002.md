# Snapshot names and fragmented guest messages — 2026-10-02

Classification: deterministic reproductions and tests (evidence rank 3).
The [capability registry](../../../capabilities/windows-hvf.json) owns current
criteria, product state and defect wording. No Windows boot, live gate or
release promotion is added. A9, A11 and A19 remain OPEN.

## Snapshot identity and receipt namespace

An isolated CLI reproduction created `before upgrade`, selected its synthetic
overlay, then created `before-upgrade`. Both operations returned success and
the catalog retained both names, but their normalized storage paths were the
same. The second operation overwrote the first disk metadata and produced an
overlay whose backing path was itself. The preservation assertion failed;
that failed experiment is not counted as a successful snapshot.

A separate reproduction used distinct names `base` and `base-create`.
The creation receipt for `base` occupied the same path as the disk metadata
for `base-create`, overwriting it. Filename suffixes also caused legacy chain
and clone readers to confuse legitimate snapshot names with creation receipts.

Admission rejects empty normalized names and catalog aliases under the existing
metadata lock before publishing catalog or snapshot files. Existing metadata
occupancy also refuses publication, preserving legacy receipt files that used
the old suffix namespace. Metadata getters require the exact stored display
name, so an uncataloged alias cannot activate another snapshot through
`snapshot disk-create`.

New creation receipts live under `metadata/snapshot-disks/creates/<slug>.json`.
Legacy root records are read by their JSON type instead of filename suffix.
Full clones rebase both old and new receipt disk paths; linked clones discard
the snapshot metadata subtree. Legitimate distinct names such
as `base` and `base-create` work in either creation order. Corrupt records
still fail explicitly. This does not repair bytes already overwritten in an
old colliding catalog or claim protection from arbitrary hostile filesystem
races. This is the generic snapshot/Compatibility storage scaffold, not new
proof of the Windows HVF disk-plus-vars interruption criterion.

Correction: the initial assessment that generic bundle imports also rebase
paths was wrong. The first complete storage suite passed 84 tests and failed
two new import assertions (retained log SHA-256
`8e718e3d5d133eac40580af305f9589bb06775b4d3067e02206550439b232787`).
The existing import path preserves copied metadata and does not invoke the
clone rebase operation. Import regression assertions now check preservation
of the distinct metadata and old/new receipt bytes, the behavior relevant to
this namespace change. The original failed assertions remain failed evidence.
Safe relocation of active/snapshot paths during generic bundle import remains
a separate open development item; this change does not implement it. Native
Windows installed-disk import/T19 is a different path.

## Persistent bounded guest-message framing

A Unix socket reproduction sent half a valid heartbeat, allowed the host's
25 ms read timeout to fire, then sent the remaining bytes. The original
stateless reader consumed and discarded the prefix. The next read returned a
JSON error instead of the heartbeat; the reproduction exited 101. The earlier
handshake-only protection did not protect active session drains or command
result waits.

The active daemon now retains one bounded framing state across drains and
command waits. Idle reads preserve the consumed prefix; complete frames leave
the next frame available. The existing byte limit includes the newline.
Truncated EOF, invalid UTF-8/JSON and oversized frames remain terminal errors;
interrupted reads retry. The blocking convenience reader remains available.

Real Unix socket regressions exercise heartbeat drains and a command result
whose prefix survives an expired wait before a later wait completes. These
prove host framing behavior, not live guest integration or responsiveness.

The retained unchanged-baseline heartbeat failure log has SHA-256
`185b0be34e25b8d27811f39c4f3094934818351f61f1044d3ce522fc8ba82d50`.
Initial reproduction-harness setup errors (a missing dependency and an
incorrect heartbeat construction) were repaired before reproducing the
product failure; they are not product defects. A test-only Clippy byte-slice
style refusal was also corrected before the focused lint pass.

## Verification checkpoint

Transport source commit: `ac31ad19`; combined source commit: `dbdf6082`.

- Agent framing: 40 tests, including seven new cases for idle retries,
  UTF-8/CRLF splits, interrupts, size bounds, malformed input and truncated EOF.
- Daemon: 75 tests, including two new real Unix socket integration regressions.
- Storage: 86 tests, including twelve new identity/namespace, concurrency,
  legacy receipt, clone, metadata preservation and refusal regressions.
- Three snapshot CLI/socket smokes, strict Clippy for all three affected crates
  and workspace formatting passed. Independent static review found no required
  follow-up in the corrected naming/framing scope.

The unchanged-baseline storage reproduction first failed all seven extended
name tests (log SHA-256
`203d8c070ad0b3ae71deacc6dd42d0a7dd6a0b1b0bd22bf0387e68705c28b1e1`);
the added legacy-occupancy reproduction failed separately (log SHA-256
`fa68dfeb38b7b0eda615fbae46a4287913ed417b9a2314218a49521c4cf55fe0`).
These failures remain recorded along with the intermediate import-assumption
failures above. The final focused storage PASS log has SHA-256
`68a5a6c1337c164cc26182c448f1fca6242062f74ec1ab95691259ed175160f0`.

Complete exact-source local and hosted checks are pending at this source
checkpoint. Previous green checks apply only to their own commits. No
threshold or existing structural ceiling is increased. New modules are
registered at their actual counted size; extracted-file ceilings decrease.
