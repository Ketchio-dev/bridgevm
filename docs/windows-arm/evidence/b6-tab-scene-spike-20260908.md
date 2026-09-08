# B6 tab scene spike: File Explorer's Home tab renders legibly (2026-09-08)

## Question

B6 requires 3/3 caption+menu+**tab** glyph passes at three declared
resolutions and three declared scales. Every retained scene so far is
classic Notepad, which has no tab strip, so tab glyphs remain untested, not
reproduced-defective. Before building the declared matrix harness, this
spike answers: does any in-box app on this exact guest image expose a
legible tab strip?

## Method

A private, uncommitted variant of `windows-1.0-closure-interact.sh` reused
the proven F1 (driver load) and F2 (1600x900 resize) steps unchanged, then
launched `explorer.exe` via the new guest asset
`scripts/win-assets/bv-b6-explorer-launch.ps1` instead of Notepad, captured
the active IOSurface, and ran Tesseract OCR. This is diagnostic-only: no
shipped script or acceptance gate changed.

A first attempt hand-rolled the launcher invocation from scratch and stalled
at `BdsDxe: starting Boot0002 "Windows Boot Manager"` on every try, with and
without the boot-progress-kill watchdog. Root cause not identified; that
path is abandoned. Reusing the actual proven script's launcher invocation
verbatim (only swapping the launched app) booted correctly on the first try,
which localizes the earlier failure to something in the hand-rolled
invocation rather than the media, disk clone, or environment.

## Result
Live run: F1 pass, F2 pass, `explorer_capture=present`. Capture
`b6-explorer-scene.ppm` SHA-256
`c4bac774a4e03b8856c54471f6567e05e3fe90b672949c0cd0ff64f2c5ef54e1`, at
1600x900 with `LogPixels` absent (effective display scale unmeasured; do
not read this as proven 100% display scale). Full-frame Tesseract OCR:

> Home Ne 2) Hore Ay Gallery > @ OneDrive GM Desktop + L Downloads # 3
> Documents # Pictures 4 @muic + Ei Videos # > This pc > Wh Network 6 items
> @ > Home Quick access ... Desktop Stored locally + Pictures Stored
> locally + YY Favorites ... Views SF Filter ... Shared ... Search Home
> Documents Stored locally + Videos Stored locally + ... Details

`Home` is the first recognized token, matching Windows 11 File Explorer's
default tab label, and the surrounding nav-pane chrome (Desktop, Downloads,
Documents, Pictures, Music, Videos, This PC, Network, Quick access, Search,
Details, Views, Filter, Shared) is legible throughout. A focused OCR pass
on an isolated top-band crop returned nothing usable; the full-frame pass is
the retained evidence, consistent with how F4 evidence has been read
throughout this project.

## Conclusion (narrowed 2026-09-08 — tab legibility only, scale unmeasured)

File Explorer's default "Home" tab is a live-confirmed candidate for the B6
tab scene: it renders a legible tab label without any extra setup (no
Ctrl+T, which the live input channel cannot send — only `win+r` and
`ctrl+alt+delete` are supported named modifier combinations). This narrows
one of the two open engineering unknowns recorded in `PLAN.md`'s B6 matrix
scoping section; the effective-scale side of every observation here remains
unmeasured. The other — a scriptable per-monitor display-scale
mechanism with measured effective DPI per run — remains open. This spike does not run
the declared 3-resolution by 3-scale matrix (9 cells, 3 runs each, 27 runs
total, not 81) and does not change B6's `OPEN` state.

## Retraction 2026-09-08

Any earlier reading of the `1600x900/100%` label on this capture as proving
a 100% display scale is retracted: the absent registry value and the
classic Notepad status-bar `100%` (document zoom, not display scale) do not
prove display scale. Failed-attempt history above is preserved; only the
scale inference is withdrawn.
