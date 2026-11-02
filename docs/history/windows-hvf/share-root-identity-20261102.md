# Bind host-derived guest deletions to the original root (2026-11-02)

A missing share root already deferred scanning, but an empty replacement at
the same path was accepted as a complete deletion snapshot. The resident
service could then delete synchronized guest files. This was a retained open
case, not an observed live incident in this change.

The share runtime now holds an opened root directory and its device/inode.
The initial selected symlink target is followed. A successful first bind is
retained; a changed target is not silently adopted. Checks bracket traversal
before scratch, unseen and failed-destination state is committed.

A queued host-to-guest DEL rechecks the root before emission and remains
queued when it is unavailable or different. Restoring the original target
recovers; actually emptying the same root still deletes the guest copy.
Shared state/constructor and traversal/DEL helpers were extracted under
existing budgets. Prior fixture assertions and samples remain intact.

The original four-case run returned exit101: one pass and three failures.
Persistent replacement and symlink-target swap removed modelled guest data;
a queued DEL also escaped replacement. Genuine same-root deletion passed.
After repair, all 271 focused clipboard/share cases passed, with production
example clippy warnings denied, formatting and structural budgets passing.

The tests use owned temporary folders and the existing modelled guest. This
protects the host-scan-derived guest-deletion path. Other host I/O remains
outside the claim; A-to-B-to-A swaps during a scan, descendant swaps and
concurrent leaf edits are not an atomic snapshot. No live or criterion proof.
Exact final project, hosted and required PR gates still apply.
