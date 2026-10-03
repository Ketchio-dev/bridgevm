# Empty snapshot publication and asynchronous runtime chooser — 2026-10-03

**Evidence rank: automated tests and static review.** These changes do not
establish a new live result or repair the recorded failed T17 pilot by assertion.
A19 source `c8973e3b0c285f815598e285b2927dfa0c51f9ac` is integrated as
`cca109df9af5d92680f424906daf5c576bb694ab`; asynchronous chooser source
`42e53b29c326595bd9653a49b9206884f061cff4` is integrated as
`466ff86b4906f0c5b7e408b6c5c1ffc7c7fba5ad`.

## Empty snapshot destination

An admitted existing empty snapshot directory previously required directory
exchange. The owned tiny fixture injected `ENOTSUP`/errno 45 and failed with
exit 101, retaining its unchanged empty destination and verified staged 8+8-byte
pair. Baseline log SHA-256:
`33664a5e19b0409465626ef64f051c07d01d153855ed22055f3de00b15b13a94`.
This is injected unsupported-exchange evidence, not an actual exFAT failure.

`crates/bridgevm-hvf/src/snapshot_publish.rs` now uses one same-parent rename
for admitted empty or absent destinations. A late nonempty entry makes the real
rename refuse; it never falls back to exchange. Nonempty complete generations
still use the extracted, behavior-preserved atomic exchange. Parent sync still
precedes deletion of an exchanged old generation. Rename branches do not delete
a staging pathname recreated after publication. Admission, ownership/media
leases and source-overlap checks retain their existing scope. Publisher source SHA-256:
`b331ae92f7162dbe141e4a72317aa1aee55a1707f186a205c80fc346daf849fe`.

The final snapshot subset passed 159 tests. The seven new cases include
one inert child entry.
Parent tests actually launch and reap exits 96/97 around real host rename,
verify the pair and retry, and preserve replacement staging for empty/absent cases.
Twelve existing A19 suites passed 91 tests on the same production bytes before
final test-only cleanup. Independent focused replay passed seven cases.
Snapshot/A19/peer raw log SHA-256 values, respectively:
`436b5f790acac7b6d7a3ea16677917f814e34e71c01a62b7817ec421b1bbf9d5`,
`19adbb10c67800a0d094c62447953c5e3764a1cb7258928349f4d3b375f61c93`,
`4975b3df6e918a500651631412a056ab7d78f553d781ddb184b2d0d338cab1a2`.
The formal source handoff SHA-256 is
`4623a1cf15939b96e5410f41572e612ede293b52760e91da02188efe25345ad5`.
No actual exFAT mount, its parent-directory sync, syscall-internal interruption,
power loss, guest run or noncooperating-writer guarantee was tested.

## Runtime path chooser

`apps/macos/Sources/BridgeVMControl/HvfEngine/HvfEngineView.swift` now calls
existing asynchronous `FileSelection.choose` and updates its SwiftUI binding
only after an accepted URL. Its previous synchronous `runModal` path was a
static investigation candidate after the authenticated T17 chooser deadline;
no failing runtime AX/deadlock reproduction or per-action timing was captured.
The original asynchronous helper's four baseline cases already passed.

Focused tests passed 74 Apple XCTest cases and 19 separate Swift Testing cases;
independent replay passed nine Apple XCTest cases. A deferred panel fixture
checks real SwiftUI binding updates, cancellation/no-URL preservation, directory
options and accepted Unicode/space paths. These stubs do not prove real AppKit,
AX routing, deadlock removal or the failed pilot's cause. Existing 20-second
chooser budget, semantic identifiers and final exact-path proof remain unchanged.
Runtime view source SHA-256:
`5c905290ccbed3f256601d9c282168f394f931426f5cc81367e65c9b415303fc`.
Parent/peer/baseline raw log SHA-256 values, respectively:
`5231f0e5c8e94d4cc7e89883b619b14dbef322aa96a78ff976309628c1a6d365`,
`4ac816accbe7ef369dfe15f3cff72e5db22bc8ccb5568b0234f37d239f3897a8`,
`bcff82d8f71a042b0fb581667826d2742f0ef96f0687716b45a6a64e8062dc7d`.
Formal handoff SHA-256:
`5a337a03beaeb982b911bbe4d2da752251f6e5a5940f7a1a601452da15888bcc`.
Focused source hashes match the committed bytes; those runs preceded commit
and do not bind a complete frozen compiled inventory or full/hosted result.

Full project and exact hosted checks for this new integrated source remain
pending. The [failed pilot](t17-firmware-cleanup-pilot-failure-20261003.md)
separately records historical exact 035a local/hosted success and an honest
FAILED journey. No criterion, product state or capability wording is promoted.
