# B6 glyph defect observed through the active-IOSurface channel — 2026-09-07

One `t7-windows-closure` run captured the B6 defect exactly as the registry
words it: body text and menu glyphs render, the window title is blank and no
tab strip is drawn. This is one observation at one resolution and one scale.
It does not run the 3-by-3 matrix, apply a pixel mask or measure frame time,
and it promotes nothing. B6 stays OPEN.

## Sealed inputs

- tested commit: `7f31bfc8d97ebe173823f156ba99b13e6085295b`
- job: `t7-7f31bfc8-keeprunning-b6-observation-r1`, started 20:10:35Z,
  finished 21:07:15Z
- input manifest SHA-256:
  `81c56875325f705fdf3728bb9e8449f3cd4ef129bedf6b3003dfb812e18d6e06`
- probe SHA-256:
  `4385f3817072ff94819a54ee002d18c5025779f67a2d17124789182232bbd974`
- injector SHA-256:
  `10760fb4d81998197d549ec7ce78ff0eb1d4d9457c5d4ed4cdcaee6f38318afe`,
  built from this head with `KEEP_RUNNING=1` and `DISPLAY_ONLY_FIRSTBOOT=1`;
  the corrected `bvinject.cmd` inside its `boot.wim` is byte-identical to the
  tree
- viogpu3d package tree `c46486e5…`, agent `b7820834…`, source image
  `7385d200…`, vars `bec224d2…`, virglrenderer `dc596bf3…`, MoltenVK
  `e1773b59…`
- receipt SHA-256 (private == public):
  `bcb1ae8a89d2d8a0466765b176a38fa435ec5edc5d0f1363b7472ad4e2ec468b`
- retained prepared pair: image `490ae154…`, vars `bec224d2…`
- host: Mac17,9, macOS 26.5

## What the run proved

`outcome=failed`, `pass=false`, `passes=3`, `failures=1`,
`injector_boot_observed=true`, `module_identity_verified=true`.

- **Injection.** The corrected injector planted the firstboot handoff (the
  pending flag, the `BridgeVMGpuDiagnosticsProbe6` service and the Stage1
  RunOnce were all present on the retained disk); the proof boot ran three
  staged reboots, reported `[stage4] done`, and answered
  `BVFIRSTBOOT_READY`.
- **F1 pass.** `BVF1 testsigning=True viogpu_status=OK viogpu_problem=0
  vioserial_status=OK vioserial_problem=0`; `BVF1MODE device=\\.\DISPLAY2
  current=1280x1024 modes=28 has_1600x900=True`.
- **F2 pass.** After `RESIZE 1600x900`, `BVF2 device=\\.\DISPLAY2
  current=1600x900`; 185 `SET_SCANOUT` commands with `rect_w=1600,
  rect_h=900`, all `OK_NODATA`.
- **F3 partial.** `WINLIST` found `131540 … "Untitled - Notepad"`;
  `WINBOUNDS 131540 50 60 700 500 -> OK`, `WINFOCUS -> OK`, guest-side
  `BVWINDOW hwnd=131540 exists=True pid=9088 rect=50,60,700,500
  foreground=131540`. Three `text-hex` key batches were accepted. After
  `WINCLOSE -> OK` the window was still listed, now titled
  `*Untitled - Notepad`: the typed text had modified the document, so
  `WM_CLOSE` raised the save prompt instead of closing.
- **F4 measured-visible-text.** `captures/f4-notepad-focused.ppm`, SHA-256
  `a2f0bb82e2173b71bdc2dfea3d3a60ab83312310f3f0ddbc6d7e4e7887e4ecfc`, from
  `capture.env`: `source=active-cgl-iosurface iosurface_id=505
  initial_seed=30 captured_seed=31 nonblack_pixels=1439262`. This is a newly
  presented 1600x900 frame, not the 2D scanout buffer that produced the
  all-black 2026-08-20 capture.

## The frame

Notepad occupies (50,60)–(700,500). Readable: the menu bar `File Edit Format
View Help`; the banner "A new version of Notepad is available." with its
`Launch` button; the body line `Probe ABCDEFGHIJKLMNOPQRSTUVWXYZ
abcdefghijklmnopqrstuvwxyz 0123456789 !@#$%^&*()`; the status bar `Ln 1,
Col 97 100% Windows (CRLF) UTF-8`. OCR (`captures/f4-ocr.txt`) reads the
menu and body words and no title.

Blank: the title bar carries the Notepad icon and the minimize, maximize and
close glyphs, but no "Untitled - Notepad" text; no tab strip (label, close
button or `+`) is drawn at all.

Measured on the retained pixels rather than eyeballed: the dark-pixel ratio
(all channels below 110) in the title text zone, x 90–600 and y 62–90, is
0.0000; the adjacent title icon zone, x 52–90, is 0.0235; the menu text zone,
y 93–108, is 0.0284; the body text zone, y 140–155, is 0.0962. The title area
is not black, not garbled and not boxed; it is painted background where glyphs
should be.

## What it means and what it does not

This is the reproduction the 2026-09-06 record required before any draw-path
cause could be proposed: the exact user-visible failure, on the corrected
capture channel, on a timer-fixed probe, with a properly injected guest. The
draw-path investigation can now start from this frame and this trace.

It is not B6 evidence toward closure. The criterion demands 3/3 at each of
three declared resolutions and three declared scales, a verified pixel mask
and frame time within 10% of baseline; none of that was run, and one
observation at 1600x900 at 100% says nothing about repeatability.

## Two harness findings, recorded rather than fixed here

1. **Stage-4 power-off races the closure tier.** `bvgpu-firstboot.cmd` ends
   stage 4 with `shutdown /s /t 5` unless `C:\BridgeVM\keep-running.txt`
   exists. Job `t7-7f31bfc8-fixed-injector-b6-observation-r1` (receipt
   `add1e95d…`) reached `BVFIRSTBOOT_READY` and passed F1, then every later
   command timed out because the guest powered off seconds later. The sealed
   2026-08-29 injector carried no keep-running marker either. The old
   passing injector's marker state is unknown; its pass does not establish
   the same race. This run used `KEEP_RUNNING=1`.
2. **A modified document survived `WINCLOSE`; shutdown did not finish.**
   The window remained listed as `*Untitled - Notepad` after `WM_CLOSE`.
   A save prompt is a hypothesis, not an observed dialog. The subsequent
   `shutdown /s /t 0` returned zero, but the guest stayed up until the
   3000 s watchdog cancelled the run. The live display showed only wallpaper,
   with no taskbar or dialog. These observations establish incomplete close
   and shutdown, not the precise mechanism that prevented shutdown.

## Interpretation correction

The captured menu (`File/Edit/Format/View/Help`) and upgrade banner identify
the classic Notepad presentation, not the tabbed application. Absence of a tab
strip is therefore **not evidence of missing tab glyphs**. The reproduced
defect is blank caption text despite a nonempty window title reported by
WINLIST. Menu glyphs are readable; tab glyphs remain untested. Earlier wording
that grouped the absent tabs with the reproduced defect was too strong.

## Cleanup confirmed live, 2026-09-08

Job `t7-64a38e83-discard-b6-observation-r1` ran code head
`64a38e83d4c256f9552b1442d43f08f5cbd6f7e2` with the same sealed manifest
`81c56875325f705fdf3728bb9e8449f3cd4ef129bedf6b3003dfb812e18d6e06`.
It started at 01:24:48Z and its receipt finished at 01:32:57Z.
Private and public receipt SHA-256 are both
`8124b25b777e22e87d4e7ed4e7b9fd80aadd92299f450054c7e46e5ad835242a`.

The guest reported `BVDISCARD hwnd=131502 pid=9084 stopped=True`, accepted
`shutdown /s /f /t 0` with exit zero, and terminated with
`stop: PSCI 0x84000008 (system off)` rather than the prior watchdog stop.
F1 and F2 passed, F3 remained partial, and F4 was measured-visible-text.
The receipt correctly remains `pass=false`, three passes and one failure;
cleanup did not turn a failed window-close assertion into a pass.

The second active-CGL capture advanced IOSurface seed 28 to 29 at 1600x900,
with 1,439,262 nonblack pixels. PPM SHA-256:
`d3b417846a82329112a636bf3897a635feff97a95f8b49d6a7bea821c2b03028`.
Visual inspection again shows blank caption text with readable menus and
document text. This supports a second caption-defect observation, not a tab
defect or a completed B6 matrix.
