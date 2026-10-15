# Package failure status and move selection — 2026-10-15

This follows the [caller-input and clone-vars record](autonomous-caller-inputs-and-clone-vars-20261015.md).
It records automated tests and source review. The ninth integrated candidate still requires
its own complete local and exact hosted checks. Product state, capability wording, release
criteria, thresholds and known defects remain unchanged.

## Eighth failure remains a failure

Eighth `a114121ced677ad13323ac380059d045df9a9dc3` completed its local full check on
2026-10-15 at 01:13:27 UTC: 44 PASS / 1 FAIL across 45 stages in 1,427.103 seconds.
All 5,121 tracked files, HEAD, tree and index remained unchanged. The failed XCTest shim
stage recorded an unexpected Cocoa 516 / POSIX 17 exception in
`CreateUbuntuTemplateTests.testLegacyNilBootModeStillCopiesAndRegistersTinyTemplate`.
The test tried to create `source/metadata`, which already existed.

The original [macOS 26 PR job](https://github.com/Ketchio-dev/bridgevm/actions/runs/38662866845/job/116187968820)
and [macOS 15 PR job](https://github.com/Ketchio-dev/bridgevm/actions/runs/38662866845/job/116187969043)
reported the same exception in their ThreadSanitizer shim steps. Neither retained log
contained a ThreadSanitizer race diagnostic. These are separate original job failures;
no rerun converts them to passes. All 103 original workflows are terminal: 100 succeeded
and three failed, with no cancellations. The manual CI also failed both app suites.
The [full project run](https://github.com/Ketchio-dev/bridgevm/actions/runs/38662925836)
checked out the exact eighth commit and recorded 43 PASS / 1 FAIL across 44 executed
stages, with the optional Linux target skipped. Its shim stage reported the same exception.

The preceding sixth retains its exact 45-stage local and 103-workflow hosted pass.
The seventh shell failure and the fifth local readiness and hosted T12 cleanup failures
remain in the prior records. Results belong to their recorded source and execution.

## Legacy template test consumes the shared fixture

Integrated `df6531518eb3337da1cf2869a895181d140a2a95` replaces one redundant directory-creation statement with an
assertion of the fixture's existing vars bytes. The clone-vars change had made the shared
fixture create metadata and vars; the legacy-nil test then tried to create that directory
again with intermediate creation disabled. The exact unchanged focused native test
reproduced the same unexpected exception before reaching its clone assertions.

After the test-only repair, native and actual ThreadSanitizer shim runs each passed the
same 22 methods: five Ubuntu-template, nine destination, three clone-vars and five
shape checks. The shim retained its actual sanitizer runtime linkage and had no skips.
Production code, the shared fixture, link-refusal tests and the 112-line ceiling are unchanged.
The original baseline executable and logs remain preserved. This focused result does not
replace the failed full check or establish guest behavior.

## Package commands preserve upstream failure

Integrated `07a13c409963a15d072426bc5277073e6e2981fa` wraps the existing package-install
pipeline in Bash with `pipefail`. Previously the final `tail -25` could succeed after
an upstream failure, causing the model to display installation success. Name validation,
argument quoting and the last 25 combined output lines are retained.

The actual generated command and model completion were tested using an owned command
fixture and a fake backend. Three of four original methods failed with four assertions;
the success control passed. Repaired native and actual shim runs each passed the same
nine methods. An upstream exit 100 and a missing command no longer become success.
Missing Bash exits 127 before the fake privileged command runs. Bash is an explicit
prerequisite; its availability in every arbitrary Linux guest is not established.
No real package manager, privileged command, SSH session or guest was exercised.

## Move completion preserves navigation made while moving

Integrated `35067ec4631bb759a731ad4963823d20655dcfca` reuses the existing clone completion's selection check through
an eight-line helper. A successful move reloads inventory and reveals its result only
when the current selection still equals the selection captured at admission. Existing
cache eviction, transfer arguments, admission, media lease and failure reload are unchanged.

The baseline exercised the actual scheduled model action, source-bound lease helper,
protected transfer, persisted registration and completion with tiny invented media.
Two of four methods failed with four assertions: selecting another VM or New Import
before completion was overwritten. Transfer and persistence checks passed, as did the
unchanged-navigation and occupied-destination controls. Final native and actual shim
runs each passed the same ten methods: six move cases and four existing clone controls.

This is a point-in-time identity comparison, preserving existing A→B→A and unchanged
context-menu-selection behavior. It does not track every intervening navigation event.
LibraryModel's ceiling decreases from 160 to 158; new files use their actual sizes.
No physical clicks, concurrent race, guest, Keychain or private media were tested.
An earlier compile was interrupted before tests by overbroad root coordination wording;
its exit 130 remains a separate harness interruption, not a product failure. The actual
failed baseline and its binaries remain. The initial inactive-sidebar attribution was
corrected to the active workspace-list binding before execution and retained separately.

## Evidence bindings

This table omits private paths and media; bound external records retain raw failures and source mappings.

| Record | SHA-256 |
| --- | --- |
| Eighth local full result | `f1cace98e2fcfa1ffc8e74f169752c91021eb3657e54b2cc0f9780d1f2d3ce4b` |
| Eighth independent failed-full review | `96cec7f485fccbf90ec302226ed046d7ef32621d006f32ab6e4f509d6cdc7abe` |
| Eighth original hosted failure analysis | `3bc90510cce8edf0f5a7801d26234f05fec0c7635401462dea81efe925714c45` |
| Legacy template fixture source/proof receipt | `0b87f041db568b5c134677b1f597666bf0bfdadeddcbe424a530169265ccf3ca` |
| Legacy template fixture independent commit binding | `d6d93e6203a82cd6f343fe7457ed2099a569d0cafc1a7771239d0c32a687b19e` |
| Package outcome report | `c0a92c282e4c3aa3fea0c5bd20feb489c31784ed648fa0dee2d48cd15ec4fa77` |
| Package outcome independent commit binding | `101100f972ad80565db8e414118635a60461a5fef3f1278411df30e074003002` |
| Move selection report | `f2a2ece4a5e03fc111f7f96635b373791e502ecbeab9f335535e6511869b5da0` |
| Move selection independent commit binding | `f0f6d30e6212a91354a8ed99fc453b49a80bc29cb8042f502979c15424aa6a11` |
