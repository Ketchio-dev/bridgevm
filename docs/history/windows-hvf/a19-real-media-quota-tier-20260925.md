# A19 real-media byte-quota tier — deterministic checkpoint

Classification: historical tier contract. Current A19 wording and product state
come from `capabilities/windows-hvf.json`. A19 remains OPEN. No T21 physical-Mac
run is claimed here.

## Measured boundary

The `t21-a19-quota-refusal` tier is designed to measure one explicit byte-quota
case using the packaged release `snapshot_pair_cli` and the private installed
Windows disk with its matching UEFI variables. It uses the same exact-source
seven-input manifest authentication and APFS clone preparation as the T20
native snapshot pilot. The queue seals the commit, manifest and release probe;
the runner uses the helper inside the authenticated, cloned app tree.

The first create request sets its quota to one byte below the selected pair's
size. A passing receipt requires exit code 1, the exact quota error, no
snapshot destination or staging tree, and unchanged disk and vars hashes.
The second request sets the quota to the exact pair size and must create and
verify a snapshot whose manifest and file hashes match both inputs. The worker
deletes its private clones and snapshots before it can report cleanup.

The strict receipt binds the job, exact commit, manifest and binary hashes to
the queue job record and immutable ledger before publication. If cleanup is
unproven, the queue retains a cleanup fence and sealed worktree. The published
receipt contains hashes, byte counts and typed outcomes only.

## Evidence limit

This helper-direct quota test does not exercise an app UI refusal. The app's
normal snapshot plan sets quota to its logical disk and vars size; the helper
uses the selected managed generation's actual size. The tier adds zero guest
boots and zero product lifecycle samples. It does not cover interrupted
create/restore, physical power loss, all quota cases or the remaining A19
sample count. Every promotion flag remains false, and Engineering Preview
wording stays in force.
