# A9 r57–r81 on the Studio — from the chooser block to the first pilot pass

Classification: physical diagnostics and one passing, nonpromoting pilot. A
pilot is never criterion evidence. The current A9 criterion, defect wording
and product state come from `capabilities/windows-hvf.json`. A9 remains OPEN
and the product remains ENGINEERING_PREVIEW.

All runs used the Mac Studio (Mac16,9, macOS 26.7) with development-signed,
exact-source builds. Each run below is a live single run.

## Pilot pass (r81)

Job `t17-f3613624-studio-selected-final-r81` ran source
`f36136249c1a7ca7335102dc471c8d9959962a2c`.

- **Time:** 01:51:53 to 02:15:51 UTC on 2026-10-02.
- **Input manifest:** SHA-256 beginning `aa74c416`.
- **Receipts:** byte-identical, SHA-256 beginning `c2828c9d`. They pass the
  strict read with `valid=true`, `pass=true`, `passes=1`,
  `campaign_mode=pilot`, `claim_eligible=false` and `criterion_pass=false`.
- **Lane result:** every journey stage is true, from artifact preflight through
  second shutdown, including the snapshot restore. `three_d_injection` is
  false, as in every 3D-off lane.
- **Console:** unlocked in all 50 samples. The product app was frontmost for
  its whole life; TextEdit was in front only before launch and during cleanup.
- **Responsiveness:** a once-per-second AX ping with a 1 s timeout never failed.
- **T19 handoff:** refused with "installed disk differs from authenticated T17
  output". See the follow-up below.

## What blocked earlier runs, and what changed

1. **ISO chooser (r57–r69).** A read-only AX probe ran during r68 and r69. It
   found two nodes whose reads fail while the open panel is up:
   - the Help menu search field `_SC_SEARCH_FIELD` (`AXChildren`, -25200);
   - the column-view preview text area (`AXIdentifier`, -25200).

   The panel was otherwise readable, and so were Finder and TextEdit. Because
   r56 passed and r60 failed with the same build, the trigger was host state,
   but it was not identified.

   Refuted: the clipboard (r61), the Korean IM extension (r66) and a reboot
   (r67). The fix walks an application root through `AXWindows` only, and
   looks for the Go To sheet among the panel's direct children. r70 passed
   the chooser.
2. **Main-thread stalls (r70–r78).** An AX ping and `ps -M` measured stalls of
   2 to 72 s at 100% user CPU, and those stalls caused the helper's -25204.
   -25204 stays not retryable: an unresponsive app is a defect, not a
   transient. Stack samples came from diagnostic `get-task-allow` re-signs,
   which are never evidence. They showed two causes:
   - **r76:** SwiftUI accessibility focus updates walked a responder per
     event-feed row (`.textSelection` on up to 500 rows). This needs an
     attached AX client, which here is the helper; VoiceOver would trigger it
     too.
   - **r78:** CoreText measured the whole feed several times per layout,
     because the pane had only a minimum height.

   The feed is now one bounded selectable text (the newest 200 events, lines
   cut at 240 characters) in a 220 pt pane. r79, r80 and r81 ran 19–23
   minutes each without a stall.
3. **Display click (r72).** The press went `legacy`, and the run log had no
   INPUTCAPS exchange; MacBook r51 looked the same. The press outcome now
   records router ownership, negotiation state and attachment. The click
   passed in r73 and in r79–r81, so the r72 cause is not established.
4. **Restore check (r79, r80).** `LockedPair` documents that the original disk
   and vars paths keep their bytes and that readers must use `paths()`. Both
   the helper and the host authenticator hashed the originals. Both now hash
   the selected pair with `snapshot_pair_cli digest`, run from the sealed app.

## Retractions and mistakes kept in the record

- **r59:** I called a run "past the chooser" from elapsed time. That was
  queue time.
- **r62:** a diagnostic suffix broke the transient-retry classifier.
- **r66:** the run was submitted before the killed process was confirmed gone.
- **r75:** stable feed-row identity (b6a52d9e) did not stop the stall and was
  reverted. The r76 stack named the real path.

## Follow-up

T19 source retention (`retain-windows-import-source.py`) copies the original
disk path, so after the journey's final restore it refuses. It runs only with
`RETAIN_IMPORT_SOURCE`, and must retain the selected pair instead.
