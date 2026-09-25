# A19 controlled interrupted-restore tier — deterministic checkpoint

Classification: historical tier contract. Current A19 wording and product
state come from `capabilities/windows-hvf.json`. A19 remains OPEN. No T22
physical-Mac result is claimed here.

## Declared observation

The `t22-a19-interrupted-restore` tier prepares a private disk and matching
vars clone from a strict seven-input signed-release manifest. It uses one
independent lane and authenticates the app CLI, bundled snapshot helper,
release probe, source commit and input hashes. The helper is invoked directly
only for the controlled interruption; normal create, export and restore retry
use the packaged app CLI.

The guest first writes an original C: marker and naturally powers off. The
app CLI creates its powered-off snapshot. A second boot writes a distinct
clobber marker and powers off. The tier starts the exact bundled helper to
restore the snapshot into a private managed pair. It accepts a stop point only
while the helper is alive, all three staged files are present, the old pair
is still selected, and `lsof` shows that exact child holding an open read FD
on the staged disk after the staged manifest exists. The authenticated source
orders three file `sync_all` calls before that read. The tier confirms the
stopped child and old selection, kills and reaps only that child, and retains
the bounded FD observation privately.

A fresh packaged app CLI export must select the exact prior disk and vars
hashes. Boot three must read the same clobber marker and shut down naturally.
The normal app CLI restore retry and a second export must select the snapshot
pair. Boot four must read the original marker and shut down naturally. The
strict public receipt contains hashes, counts, typed outcomes and cleanup
state; guest strings, paths, raw FD logs and media stay private. A missed
stop point or pair/marker mismatch reports zero completed cases and samples.

## Evidence limit

This is one controlled process death before publication, not sudden physical
power loss. It does not test the staging-directory sync, every interruption
point, a product UI quota refusal, or the declared remaining A19 lifecycle
sample count. Any future single passing T22 result remains nonpromoting:
`claim_eligible`, `criterion_pass` and `capability_promotion` stay false.
