# Descriptor-bound guest share destinations (2026-11-02)

Host-scan root checks did not cover resident GET publication or guest-originated
host deletion. Those operations reopened the current root path, so a delayed
valid reply could overwrite or unlink an unrelated same-name replacement file.

The selected root is now checked before adopting guest bytes or deleting a
host file. Its held live directory descriptor is cloned and supplied to a
Destination factory that walks relative to that descriptor. A later root-path
replacement cannot redirect the operation to the replacement folder.

Unavailable, changed or unlinked roots defer without false baseline progress
or permanent destination disablement. Matched GETEND still completes its wire
slot; restoring the original target retries the same version/deletion.
Component/NOFOLLOW rules, ACLs, permissions, quarantine, atomic replacement
and leaf unlink retain their existing implementation. No unsafe was added.

The old-production service regressions returned exit101 with two failures:
GET overwrote the replacement sentinel and local deletion removed it. After
repair, 274 combined clipboard/share tests passed, including the earlier271,
those two refusal/recovery cases and a direct verified-FD lifetime case that
replaces the root path before destination creation. Production clippy with
warnings denied, formatting and structural budgets passed. Existing4095
failure-budget samples keep their assertions through an owned-root fixture.

These are synthetic owned-file results. The claim covers resident guest-to-host
share write/delete destinations; host uploads, path-based scans, descendant
races and universal filesystem atomicity remain outside it. Initial selected
symlinks keep their pinned-target behavior. Exact final gates remain required;
criteria, limits and product state stay unchanged.
