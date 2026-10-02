# T19 selected final media correction — 2026-10-02

Classification: deterministic reproduction and correction at
`18a5bab872f90bf27607a5ed963fb74593fd4e84`. No live Windows run,
release gate receipt or capability promotion. Earlier failed experiments remain
in `snapshot-media-and-evidence-followup-20261002.md`.

## Confirmed stale-path evidence

The installed-disk import journey performs a managed-pair snapshot restore.
Its Swift final evidence and T19 host authenticator nevertheless hashed the
logical original disk and vars paths. Those originals retain their old bytes;
the next boot selects the restored generation under the managed root.

A synthetic pair exercised the real product `snapshot_pair_cli`: create,
change both originals, then restore. Both original hashes differed from the
selected hashes. Running the exact previous writer from commit
`9f3dea8cbe6da0d7dc7acf1ffdd06d0314874476` against that pair authenticated
the stale original hashes and created a host stamp. This failed experiment
is retained as the reason for the correction; it is not guest behavior evidence.

## Correction and focused verification

Swift final import evidence now calls the existing `T17SelectedMedia.digest`.
The host authenticator uses the same selected-pair digest protocol through the
helper inside the request's app bundle. Request bytes still require their
prelaunch SHA-256; exact identity types, fixed lane paths, source comparisons
and app manifest checks remain enforced. Missing or symlinked packaged helpers
cannot produce a stamp. The existing lane result encoder was extracted intact
so the evidence module remains within its structural ceiling.

The new real-helper regression passed two tests: stale original hashes are
refused, selected hashes authenticate, logical originals remain unchanged, and
missing or symlinked helpers are refused. The existing eight request/identity
cases passed. Five focused Swift tests passed, including failure preserving
both previous final hashes. The signed LaunchServices fixture smoke passed
its pilot and bad-hash/request-rewrite/request-append refusal cases with
verified cleanup. Its app fixture packages the pair helper before sealing.
These are deterministic checks, not a Windows acceptance run.

## Full-check checkpoints

The preceding commit `9f3dea8cbe6da0d7dc7acf1ffdd06d0314874476` passed
local `scripts/check-project.sh` after the earlier sandbox restrictions were
removed. The full log SHA-256 is
`d2c235ed1ffa8397bae722fe0dc8f5d586b35cc47b933293130b1e8270d572f2`.
That local result does not cover this later correction.

Hosted full-check run `37024645981` for the preceding commit was still
`in_progress` when this record was written. The latest correction's complete
local check was underway; its final exact-head hosted check had not yet been
submitted. Neither pending check is counted as passing evidence. Final results
and run identifiers belong in the PR checkpoint. Product state and all open
criteria remain unchanged.
