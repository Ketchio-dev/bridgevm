# Installed-disk import tier checkpoint — 2026-09-16

Source checkpoint: `b2a60b9f4d17c77352a003d8ac0edaecf0bdf6c6`.

This checkpoint adds the strict 3D-off installed-disk import request, the
packaged-app import runner, immutable lane-media authentication, the dedicated
T19 physical-Mac queue and receipt contract, and fail-closed synthetic normal
and tampered-lane tests.

The complete local project check executed every remaining gate successfully,
including shim suites of 425 tests, 802 tests with two required live-only
skips, and 62 tests. It then correctly failed only the capability-registry
freshness check because `tested_commit` still named the earlier source
checkpoint. This record binds that deterministic result to the source above;
the metadata-only follow-up must pass the same complete project check.

This is deterministic integration evidence only. No real Windows media was
imported or booted, no guest behavior was observed, and exact-head hosted
checks plus a physical T19 pilot remain required. The synthetic T19 smoke does
not satisfy A9.
