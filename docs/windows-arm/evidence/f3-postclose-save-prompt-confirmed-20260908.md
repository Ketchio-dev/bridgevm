# F3 post-close save prompt: confirmed by direct capture, not inferred (2026-09-08)

## What was uncertain

Every closure-tier run since `t7-7f31bfc8-keeprunning-b6-observation-r1`
(2026-09-07) has left the Notepad window in `WINLIST` after `WINCLOSE`, so
`f3_window_verbs` stays `partial` rather than `pass`. HANDOFF and the
capability registry both carried a hedge: "a hypothesized save-prompt veto"
/ "the inferred save-dialog/shutdown-veto mechanism ... too strong" — the
window's continued existence was consistent with a standard Windows
"save changes?" prompt, but no run had actually looked at the screen between
`WINCLOSE` and the guest-side discard to check.

## What changed

`windows-1.0-closure-interact.sh` (commit `5261db56`) now captures the
active IOSurface and runs OCR on it in that exact window, purely for
observation; it does not change `f3`, the discard call, or the forced
shutdown that follows.

## First attempt was invalid, and is retracted

`t7-c4e1a6ee-f3-postclose-r1` was submitted with `--sha c4e1a6ee`, one commit
before the capture code landed (`5261db56`). `git show c4e1a6ee:scripts/windows-1.0-closure-interact.sh`
confirms the checked-out worktree had no `capture_active_scanout` call in
this branch. The run reproduced the ordinary F1 pass/F2 pass/F3 partial/F4
measured-visible-text result with no capture files anywhere under its
`proof/captures/`, which is expected for the old script, not a new failure.
It contributes nothing to this question and is not cited below.

## Corrected run

`t7-5261db56-f3-postclose-r1`, submitted against commit `5261db56...` (the
same code head, resealed as `5261db56` after this doc; see registry),
2026-09-08 04:49:57Z-04:58:26Z. Receipt: `f1_driver_load=pass`,
`f2_resize=pass`, `f3_window_verbs=partial`, `f4_glyph_observation=measured-visible-text`,
`outcome=failed`, `pass=false`, 3 passes / 1 failure — identical closure
outcome to every prior run; this instrumentation changes no result.

`proof/captures/f3-postclose-dialog.ppm` SHA-256
`6e199478002ca2206d16da3a751ad43243138190ae9c65b8fbe31e7cc37cfc08`. Tesseract
OCR of that frame (`proof/captures/f3-postclose-ocr.txt`):

> View Help  Untitled - N  File Edit Format  Anew version of Notepad is
> available, 1 Probe ABCDEFGHIJKLMNOPQRSTUVWXYZ  Launch
> abcdefghijklmnopqrstuvwxyz 0123456789 !@#$%^&*() «|  **Notepad  x  Don't
> Save**  UTES  Ln, Col 97  100% Windows (CRLF)

## Conclusion

The OCR directly reads a second window titled **Notepad** (not
*Untitled - Notepad*) carrying a **Don't Save** button and its own close
control — the standard Win32 "Save changes to Untitled?" prompt, layered
over the still-visible main Notepad frame (menu bar, typed alphabet/digit/
punctuation body text, and status bar are all still legible underneath).
This is **directly observed**, not inferred: `WINCLOSE` correctly delivers
`WM_CLOSE`, Notepad correctly raises its save prompt because F4 left the
document modified, and the window is never destroyed until that prompt is
answered. `WINLIST` still reporting the hwnd afterward is exactly correct
Win32 behavior, not a Coherence-protocol defect. The forced-shutdown +
guest-side discard path already handles this case (`BVDISCARD hwnd=... stopped=True`
in every one of these runs), so no code change follows from this finding.
Retire the "hypothesized" / "too strong" hedges in HANDOFF and prior evidence
docs: the mechanism is now confirmed, not guessed.

This does not promote any criterion. `f3_window_verbs` is `partial` by
design when the prior interaction leaves the document dirty; it is not a
release gate on its own account.
