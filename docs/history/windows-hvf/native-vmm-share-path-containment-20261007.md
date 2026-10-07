# Native VMM shared-folder path containment — 2026-10-07

Integration starts from main `a0515594`.

## Defect

The product runtime enables shared-folder sync. It passes
`BRIDGEVM_VIRTIO_CONSOLE_SHARE` as `<host dir>::<guest dir>`. The host polls
the guest agent with `LSR`, and the agent replies with a base64 listing of
`relpath|size|isDir|mtime` lines. The guest controls those relative paths.

`parse_ls_into` collapsed separators and dropped `.` components, but it kept
`..`. The sync engine used each listing path as a share key, and the host
joined that key to the share root:

- A new guest entry made the host issue `GET`, then write the reply to
  `host_dir.join(name)`, creating parent directories first.
- A recorded key missing from a later listing made the host call
  `remove_file(host_dir.join(name))`.

A guest listing `..\escaped.txt` therefore made the VMM process write a
guest-chosen file one level above the share root. A deeper `..` chain reaches
any path the user account can write. The fixture reproduced the write outside
the share root on the original code.

## Repair

`from_guest_rel` now returns `None` for a listing path with any `..`
component, and `parse_ls_into` drops such entries. They never become share
keys, so they cannot reach `GET`, `PUT` or `DEL`, or any host-side join.
Separators are collapsed before joining, so a leading `\` or `/` cannot make
the key absolute. A name that only contains dots, such as `..ok.txt`, is
still accepted.

Host-scan keys come from `strip_prefix(root)` over a symlink-free walk of the
host share, so they were already contained.

The relative-path helpers moved into `share_rel_path.rs`, and
`share_sync.rs` drops from 738 to 717 lines.

## Evidence

`guest_listing_entries_cannot_name_paths_outside_the_share` passes only
`abs/..ok.txt` out of four hostile listing lines, and the sync engine plans no
action for `..\escaped.txt`. With the check removed, the test fails and keeps
`../escaped.txt`, `sub/../../x` and `../up`. The `hvf_gic_boot_probe` example
tests pass 493/0 with and without the `venus` feature, and clippy is clean.

## Scope

Shared-folder transfer, conflict and size behaviour are unchanged for paths
inside the share. A11 remains OPEN. Capability states, thresholds, known
defects and product wording are unchanged, and there is no live Windows or
release claim.
