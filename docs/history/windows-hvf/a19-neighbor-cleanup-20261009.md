# T20/T21 cleanup owns allocations, not names — 2026-10-09

Evidence rank: deterministic tests and source review, not live guest evidence.
This follows the T22 cleanup repair without changing its frozen evidence.

Actual runner subprocesses on disposable output directories, with absent seals
and no private inputs, exited one after seal refusal but deleted a pre-existing
`prepared-inputs/sentinel`. T21 also deleted a pre-existing quota snapshot tree.
Both incorrectly reported `worker_cleanup_verified=true`. Normal untouched queue
submission allocates a fresh job; this reproduction concerns reused output or
residue and does not imply foreign users can modify a private job directory.

T20 now records prepared-directory ownership through the existing allocation
callback and reuses identity-bound cleanup. Existing/replaced paths remain
untouched. A present live path, signaled lifecycle, or interrupted launch/wait
keeps media and unverified cleanup. A successful sample still requires the
unchanged authenticated marker evidence and three natural shutdowns.

T21 runs snapshot, staging and lease operations inside its allocated preparation
tree, outside the authenticated app bundle. Cleanup never adopts loose legacy
output names; all six names remain checked for residue. Partial owned trees can
be cleaned, but replacement or quarantined cleanup failure stays unverified.
The exact one-byte-under refusal, accepted quota, manifest and pair hashes are
unchanged. Historical receipt validators and worker-fence contracts are intact.

Root checks passed: T20 cleanup ten tests, shutdown accounting three, the full
T21 contract nineteen and archive reader fifteen. The initial T20 fixture
intercepted real OpenSSL hashing incorrectly; its failed log is retained and the
corrected fixture delegates unrelated subprocesses. A provider interruption
returned no T20 patch before the successful bounded follow-up.

A separate cached debug helper accepted tiny sibling inputs and destinations
under one preparation parent: quota seventeen refused an eighteen-byte pair;
quota eighteen created and verified the exact pair. This is compatibility signal
only, not sealed release-binary provenance or real-media quota evidence.

No existing structural ceiling was raised. Exact sealed local full-project and
GitHub-hosted checks remain required. No private media, actual live queue,
permission, worker service or fence was changed. A9/A11/A19 remain OPEN and no
release, lifecycle sample or product promotion is claimed.
