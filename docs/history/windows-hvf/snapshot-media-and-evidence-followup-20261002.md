# Snapshot media and evidence follow-up — 2026-10-02

Classification: deterministic tests and failure analysis. No live guest run,
release gate or product-state promotion. Earlier results remain in
`snapshot-retention-evidence-and-input-deadlines-20261002.md`.

## Confirmed follow-up failures

Snapshot creation could publish a manifest larger than the new 64 KiB reader
accepted. Creation now checks encoded metadata before staging and again before
publication. A restore lease-extension error also left its staging generation;
ownership-based cleanup removes only that attempt's directory while preserving
selected media, original paths and open-reader bytes.

A valid manifest did not make its media safe to open. A FIFO disk member blocked
verification past a two-second subprocess timeout; a symlink with matching bytes
was accepted. A shared descriptor opener now uses `O_NOFOLLOW | O_NONBLOCK` and
requires a regular file before reading. Verification refuses an unexpected
length before hashing and reads at most the declared length plus one byte,
rejecting growth or shrinkage. Logical aliases remain supported by selected
pair digest after canonical selection. Synthetic disk and vars tests also verify
that failed admission leaves original and selected media intact.

T19 authenticated postlaunch request bytes without requiring their prelaunch
hash. The tier now seals its request before launch and the writer requires that
seal. T17/T19 fixed identities require their exact JSON types; Boolean/floating
lane numbers, numeric Boolean flags and nonstring hashes are refused. Recovery
package and code paths must belong to the declared lane.

The runtime event display drew over 100,000 lines from one command. It now draws
only the newest 200 hard lines without changing retained events. A subsequent probe
showed that one grapheme containing 1,048,576 combining marks still passed the
240-character cap and returned 2,097,153 UTF-8 bytes. Its byte/scalar-bound
correction limits each displayed line to 1,920 scalars before applying the
240-grapheme cap. The million-mark reproduction now returns 3,842 UTF-8 bytes;
the four-byte scalar case returns at most 7,683 bytes including an ellipsis.
Seventeen focused event-feed tests passed. Unicode hard line separators are
normalized for display, with CRLF coalesced to one boundary. A synthetic AppKit
probe had also shown CR, LS, PS, NEL and FF bypassing the line counter. After
the correction, LF, CR, LS, PS, NEL, VT and FF each produced 200 normalized
segments, 399 synthetic characters and 2,600 pt of attributed height, matching
the LF baseline. Full retained events remain byte-identical.
No CoreText timing or live UI responsiveness is inferred from these tests.

## Hosted failures and corrections

Initial full check `37015928755`, at
`835c9898672a92fb1948aec6af89775528731ca9`, failed the read-only retained-directory
cleanup test; every other full-check section passed. Dedicated snapshot runs
`37015844261` and `37015994314` reported success but contained that same failure.
Their shell `&&` lists allowed a later successful line to mask it. Those green
statuses are retracted. The extracted check script now exits at the first failed
suite; a controlled failing publication command also prevented all later suites.

Diagnostic head `7ebd2825a9c3fdba96b972ad648a0f8c6934f462`, PR run `37020998241`,
identified the actual operation: quarantine `renamex_np` on the locked `0500`
directory returned `PermissionError [Errno 13]`. Local success did not establish
hosted compatibility. The correction binds its permission change to an
opened directory descriptor matching the owned device/inode before renaming;
the locked-success and replacement-preservation criteria remain unchanged.
Cleanup now opens that descriptor before quarantine, checks ownership, changes
only its inode to `0700` and holds it through clearing. Eleven publication,
cleanup-race and permission tests passed locally, including simulated Darwin
rename refusal and replacements before and after descriptor opening. Hosted
verification of this correction remains required.

Security/quality run `37015994248` failed fuzz smoke because its independent
lockfile lacked the new parser dependencies. The lockfile was corrected; local
fuzz smoke then passed four targets, 373 inputs and zero crashes. This result
provides deterministic feedback, not guest or shipping evidence.

The initial full-check dispatch `37015843900` used the wrong source SHA and was
cancelled; `37015928755` was its correctly targeted replacement. The cancelled
experiment is not counted as validation.

## Validation and preserved limitations

The media-admission checkpoint passed 131 snapshot tests, strict Clippy and Rust
formatting. Its first test run failed two old hash-tamper tests because their
fixtures also changed the media length, now rejected earlier. The corrected
fixtures replace bytes with equal-length content and retain exact hash-mismatch
assertions; the failed 129/2 run stays recorded.

A later source-growth probe found that creation checked quota only before
copying. One hundred synthetic concurrent creates published 23 pairs of
131,072 bytes against a 69,632-byte quota (53 within quota, 24 refusals).
Creation now checks actual copied sizes before hashing or publishing metadata,
using checked addition for both source estimates and copied sizes. Deterministic
DiskSynced growth, exact-boundary and overflow cases passed; the latest snapshot
suite passed 134 tests. A supplementary repeat had zero over-quota publications,
64 within quota and 36 refusals. These concurrency counts are exploratory
reproduction data, not a live gate or proof against every filesystem race.

Current request identity tests passed eight behavioral cases. Earlier focused
chooser/button tests passed 83, selected-helper retention four and media
mutation two; publication five and cleanup races three passed locally before
the hosted permission failure. Structural ceilings were reduced or new files
registered at their actual size, with no existing ceiling raised.

The third complete local project check failed six steps affected by process
queries, LaunchServices and nested SwiftPM sandbox restrictions. Its log SHA-256
is `19e3ba3ffc115e22cf4185b35d77c44a6ffb1a374965546495479f51c330a358`.
No check was relaxed or moved to a physical-Mac queue. Final exact-head hosted
full verification is required; its run and result belong in the PR checkpoint.
All release-blocking states, known defects and ENGINEERING_PREVIEW remain as
recorded in the capability registry.
