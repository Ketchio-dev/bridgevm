# B6 single-scene observation: modern Notepad supplies tab and menu; classic caption scene retained (2026-09-08)

## Question

B6's statement reads "Caption, menu and tab glyphs pass 3/3 at each of
three declared resolutions and three declared scales." Every prior
observation used classic `notepad.exe` (caption + menu, no tab strip) or
File Explorer's redesigned command bar (tab strip, but no traditional
dropdown menu). Before designing the matrix harness, this spike asked
*whether a single scene could supply all three glyph classes at once, so
the 27 required runs would measure one scene rather than needing two
scenes per cell. (Answer 2026-09-08: no — see Correction below; the
classic caption scene is retained.)

## Method

A live query (`bv-b6-query-modern-notepad.ps1`, `-File` invocation like
every other proven asset) found `Microsoft.WindowsNotepad` package
**11.2501.31.0** already installed on this exact guest image, even though
every prior `Invoke-CimMethod ... CommandLine = 'notepad.exe'` launch in
this project observed the classic win32 presentation. `notepad.exe`
resolves to the classic binary on this image; the packaged app is reached
only through its AUMID. `bv-b6-modern-notepad-launch.ps1` reads the
package's `PackageFamilyName`, builds `<PackageFamilyName>!App`, and
launches `explorer.exe shell:AppsFolder\<AUMID>`.

## Result

Live job captured and OCR'd the result: capture SHA-256
`197e54bad9131aeea14da5dbfde2e784da1014f036e962923f09b8d9f66b6f0e`. OCR:

> untitled File Edit View Notepad automatically saves your progress. All
> your content will be available the next time you open Notepad. Got it
> Ln 1, Col 1 — 0 characters Windows (CRLF) Test Mode Windows 11 Home
> Build 26100...

Visual inspection confirms two distinct rows stacked at the top of one
window: a combined title/tab row (Notepad icon, window controls, an
**"Untitled"** tab with its own close button and a `+` new-tab button --
no separate traditional caption bar; the tab row itself plays that role in
this Windows 11 redesign), then a plain **File / Edit / View** menu bar
directly below it. The capture was taken at 1600x900 with `LogPixels`
absent (effective display scale unmeasured; do not read this as proven
100% display scale).

## Correction 2026-09-08 — tab row is not a classic caption

The tab row observed here is readable tab content, not a separate classic
caption bar. It therefore does not cover the originally failing classic
Notepad caption path: the classic caption/menu scene (the reproduced blank
title-bar defect and its fix) must be retained alongside this packaged
Notepad tab/menu scene. The B6 matrix needs both scenes per cell family,
not one scene replacing the other.

## Conclusion (narrowed 2026-09-08)

The modern packaged Notepad supplies tab and menu glyphs together in one
compact region of one window, live-confirmed at 1600x900 with unmeasured
effective scale on the fixed head. This is a tab/menu scene candidate for
the declared B6 matrix, alongside — not instead of — the classic Notepad
caption/menu scene (and the File Explorer Home tab fallback). Two new
diagnostic-only guest assets support this:
`bv-b6-query-modern-notepad.ps1` and `bv-b6-modern-notepad-launch.ps1`;
neither is wired into any shipped closure gate. The desktop watermark
visible in the capture ("Test Mode ... Build 26100...") is retained as
observed and is a pre-existing test-signing artifact of this image, not
introduced by this spike.

## Retraction 2026-09-08

Any earlier reading of this spike as supplying "caption, tab and menu
together", as "caption-equivalent" coverage, or as a single scene that
removes the need for the classic caption path is retracted. Failed-attempt
history above is preserved; only the caption-coverage inference is
withdrawn.
