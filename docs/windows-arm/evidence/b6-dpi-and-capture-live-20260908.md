# B6 effective-DPI query and presented-frame capture: proven live together (2026-09-08)

## Question

The B6 matrix needs, per cell, effective window/monitor DPI proven live plus a
fresh presented capture taken immediately after a genuine content-changing
action (the seed-advance constraint recorded in
`b6-scale-mechanism-spike-20260908.md`). Neither had been exercised together
in one run: the DPI query asset (`bv-b6-window-dpi.ps1`) had never run
against a live guest, and prior captures never queried DPI on the same
window in the same run.

## Method

A private, uncommitted harness variant (`b6-dpi-capture-spike.sh`, same
F1/F2/launch/hwnd-acquisition steps as the proven closure script) inserted
one extra step between confirming the classic Notepad window exists and
typing into it: call `bv-b6-window-dpi.ps1 -Hwnd $hwnd` on the exact live
hwnd, then proceed with the already-proven typing-then-capture sequence
unchanged. Two live attempts against stale/uncertain retained media stalled
at `wait_firstboot` (documented and discarded below); the third attempt
reused the exact retained pair and asset set already proven minutes earlier
by `t7-caption-clean-20260908-r1` (image `0bee49d3ea6b...`, binary
`e08d6017ec87...`, virglrenderer `568e1544e05d...`, all reverified by hash
before the run).

## Result

Full non-fast run, code head `1f574b65`. F1 pass, F2 pass, F3 partial
(expected: WM_CLOSE on an unsaved document raises the save prompt, per
`b6-*-observation` history), F4 measured-visible-text. On hwnd `131652`:

```
BVAGENT CMD powershell -NoProfile -ExecutionPolicy Bypass -File C:\BridgeVMClosure\bv-b6-window-dpi.ps1 -Hwnd 131652 exit=0
BVEFFECTIVEDPI hwnd=131652 dpi=96 awareness=2 monitor_scale=100
```

The verifier script's own acceptance regex expected `awareness=<name>` but
`GetAwarenessFromDpiAwarenessContext` returns a bare enum ordinal (`2` =
`PROCESS_PER_MONITOR_DPI_AWARE`), so the spike's own bookkeeping logged
`effective_dpi_query=fail` even though the command executed, exited 0, and
returned a well-formed, internally consistent answer -- a verifier bug, not
a guest-side failure. `dpi=96` and `monitor_scale=100` agree (96 DPI is
Windows' 100% baseline), and the classic Notepad status bar in the same
capture independently reads `100%` (document zoom, a different signal, but
consistent).

Immediately after, the same window received the proven typing sequence and
a fresh presented capture: `active_scanout_capture=present`, SHA-256
`73121954e720562b3b946537aa4917729421cc67d3e4bf9f1584f37ea3f837d0`, showing
`*Untitled - Notepad` with the full caption, File/Edit/Format/View/Help menu
and typed probe text all legible at 1600x900 -- the caption fix (`f1018e97`)
holding, independently confirmed again.

## What this proves and what it does not

Proven: the effective-DPI query mechanism works against a real live window,
and a DPI query plus a presented capture can both be taken from the same
window in the same run, in the order the matrix harness needs. This clears
the last engineering unknown blocking the matrix harness build itself.

Not proven: any of the declared 100%/125%/150% scale cells (this run's
`monitor_scale=100` is the untouched default, not a set-and-verified scale
change), any of the 1280x720/1920x1080 resolutions, or any pixel-mask/frame-
time acceptance. Zero of the 27 declared matrix cells have run. B6 stays
`OPEN`.

## Two prior stalls, root-caused to stale retained media, not a regression

Two attempts against older cached "windows-1.0" retained pairs
(`0ab72f8792...` and `845fec6f6a...`) both timed out at 45 minutes waiting
for `BridgeVM-VioGpu3DFirstBoot` to complete. A live diagnostic query showed
`schtasks.exe /Query /TN BridgeVM-VioGpu3DFirstBoot` returning "the system
cannot find the file specified" -- the task was never present on either
clone. Both were retained pairs from early 2026-09-07, before the caption-fix
rebuild (`f1018e97`) and before the 2026-09-07 vtimer-recovery regression was
removed (`13fb8213`); they are not known-good and should not be reused.
`t7-c193b7c0-fixed-b6-observation-r1`, cited elsewhere as a successful proof
in `HANDOFF.md`'s earlier drafts, is corrected here: its receipt actually
recorded `f1_driver_load: false` and the same stalled-firstboot pattern.
That correction and this run's success point the same direction: use only a
retained pair created by a run on or after `97c2bf4c` (the caption-fix
rebuild), never an older cached one, until the matrix harness generates its
own fresh pair per cell.

## Retraction 2026-09-08

`HANDOFF.md`'s prior text describing `t7-c193b7c0-fixed-b6-observation-r1`
as a completed proof job is wrong: its own receipt shows
`f1_driver_load=false`, `pass=false`, `outcome=failed`. That job never
reached firstboot readiness. The only genuinely proven closure runs found on
this host today are `t7-caption-clean-20260908-r1` and this one, both after
the caption-fix rebuild.
