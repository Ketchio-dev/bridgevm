# B6: PresentMon capture proven live, focus refusal characterised (2026-09-09)

One live boot on the M5 Pro (`Mac17,9`, macOS 26.5) against the retained pair
`0bee49d3ea6b...` / `bec224d2...`, with the asset set hash-matched to the
`t7-caption-clean-20260908-r1` input manifest (probe `e08d6017ec87...`,
virglrenderer `568e1544e05d...`, viogpu dir `download-b4-attachqueue-56df708`,
MoltenVK from `/opt/homebrew/opt/molten-vk`, PresentMon `9bec3083...`). Three
runs of both declared scenes at 1600x900, `LogPixels` 96. B6 stays `OPEN`; this
promotes nothing.

## PresentMon captures for real, and needed no elevation

The `--stop_existing_session` fix (PR #151, `142f8257`) works on a live guest:

```
bv-b6-presentmon-capture.ps1 ... exit=0
BVPRESENTMON path=C:\BridgeVMClosure\presentmon-classic-run1.csv rows=3
```

The CSV came back to the host with real `dwm.exe` rows (`Hardware: Legacy Flip`,
`FrameTime` 580.8174 and 495.4755 ms). Every 2026-09-08 run of the same asset
exited 7 without producing a file.

This settles the elevation question the same way the binary's own help text
did. The capture ran as `bridgevm\bridge`, not `LocalSystem`, with no
`--restart_as_admin`; realtime ETW capture did not need the privilege the
retracted reading blamed. Do not add an elevation step.

## The frame-time clause cannot be measured on a static scene

Two presents in a fifteen-second capture is not a sampling problem to be fixed
with a longer window: DWM composes when something changes, and a Notepad window
that has finished being typed into changes nothing. A `FrameTime` of 580 ms
here means "nothing happened for half a second", not "the frame took 580 ms".

B6 requires "frame time within 10% of baseline". Two samples support no
percentile and no comparison, so the clause as written is unmeasurable against
the declared scenes. This has to be settled before the campaign runs, not
after: either the scenes gain something that presents continuously, or the
clause is re-declared for scenes of this kind -- and, as with B8, the
declaration has to be recorded before the measurement.

## Focus refusal is deterministic, and it follows the packaged scene

| run | scene | hwnd | `SetForegroundWindow` | capture |
| --- | --- | --- | --- | --- |
| 1 | classic | 197060 | OK | present |
| 1 | packaged | 196900 | OK | absent |
| 2 | classic | 327696 | **ERR x5** | refused |
| 2 | packaged | 262808 | OK | absent |
| 3 | classic | 131832 | **ERR x4** | refused |
| 3 | packaged | 721332 | OK | absent |

Classic Notepad is refused the foreground on every run that follows a packaged
(UWP) scene, and never on the first. The packaged scene is granted it 3/3. The
agent answers `ERR WINFOCUS` promptly, so this is `SetForegroundWindow`
returning FALSE, not a hung channel -- the shape of a Windows foreground lock
held by the process that had it last. Releasing the foreground between scenes,
or taking it the way a user does with a real click, is the thing to try; a
click was added to the harness on this finding and is not yet proven.

The 2026-09-08 reading is therefore confirmed live: the failures recorded then
were focus refusals the harness discarded with `|| true`, not a display-engine
limit. With focus now a precondition, the refused runs record
`classic_focus=fail` instead of silently producing "capture absent".

## The stale-descriptor hypothesis fails a second time

This boot's log contains exactly one `virtio-gpu: scanout IOSurface global
id=15 (1600x900)` line and the descriptor still reads `15 1600 900` at the end,
so the active surface never changed across six scene attempts. As in the
2026-09-08 run, `capture-active-iosurface.py` was never reading a stale
descriptor. The packaged scene fails because the guest presented nothing, with
confirmed foreground and delivered input (2 pointer, 12 key commands accepted).

## The packaged scene: twelve failures, one wrong unit

A second boot the same day ran `observe-active-iosurface.py` on that scene, and
the answer was on the screen the whole time. `packaged-run1-observed`
(`observation_only=true`, 1,439,965 non-black pixels of 1,440,000) shows
packaged Notepad with its first-run tip still up -- *"Notepad automatically
saves your progress..."* over a **Got it** button -- the tab reading `Untitled`
with no modified marker and the status bar reading `Ln 1, Col 1` and
`0 characters`. The typing had gone nowhere because the tip still owned the
window, so nothing repainted and the seed was never going to advance.

The tip was not dismissed because the click never went near it. The frame also
shows a Recycle Bin tooltip open in the top-left corner, which is where the
pointer actually was. `POINTER` coordinates are HID absolute-axis units
spanning `0..0x7fff` across the screen (`POINTER_INPUT_AXIS_MAX`), not pixels;
the proven pointer campaign converts with `px * 32767 / (dimension - 1)`
(`scripts/run-pointer-click-reliability-case.sh:44`). The B6 harness passed raw
pixels, so `click:547x303` landed at about `(27, 8)` px -- on the Recycle Bin.

The guessed coordinate was not even wrong about the button: measured on this
frame, **Got it** sits at about `(544, 302)` px, within a few pixels of the
guess. It was the units that were wrong, and no amount of re-guessing the pixel
position would have found that. Twelve packaged attempts across two days failed
this way, invisibly, because the freshness gate correctly refused the frame and
nobody had yet looked at it.

## A defect in the fix itself

`focus_window` first passed `send` a pattern matching only `-> OK WINFOCUS`, so
a prompt `ERR` reply burned the full 120-second step timeout on every one of
five attempts -- ten minutes per abandoned scene -- and then reported it as "no
reply" when the agent had answered at once. Fixed to match `-> (OK|ERR)` and
branch immediately. Worth noting because the campaign is 36 boots: a defect
that only costs wall time still costs the whole budget.

## Both scenes captured 3/3, and what it took

Three further boots the same day closed the remaining harness defects. The
first run with the corrected pointer units took packaged captures from 0/12 to
2/3 and proved the click fallback -- `hwnd=327846 took foreground from the
click on attempt 1`, where `SetForegroundWindow` had been refused -- but its
own results could not be trusted, because two more defects were still mixing
the scenes together:

- **`find_hwnd` scanned the whole log.** It grepped every `WINLIST WIN` line
  the boot had ever produced rather than the reply to the request it had just
  sent, so when the current listing held no match it silently returned a window
  from an earlier one. Run 3's classic scene got the packaged window's hwnd and
  run 2's packaged scene got the classic one; a capture filed as
  `classic-run3` is in fact the packaged window carrying three rounds of
  accumulated text. Any label from that run is unreliable.
- **Teardown was assumed, not observed.** `WINCLOSE` then discard then
  `sleep 2`, with nothing checking that the window had gone -- and F3 already
  established that `WINCLOSE` on a dirty document raises a save prompt and
  leaves the window alive. `wait_window_gone` now polls the current listing
  until the hwnd is absent.
- **The packaged window was assumed to exist two seconds after launch.** It
  sometimes does not; run 2 found none at all. The launch now polls.

Packaged Notepad also restores its previous session on relaunch, exactly as its
own first-run tip advertises, so its document accumulates across runs unless
that is handled. B6 requires three independent runs, and a scene that carries
state between them does not provide that.

## Nine characters, and the space that caused them

With the scenes finally isolated, the packaged scene still took exactly nine
characters of an 84-character payload, every run, while classic Notepad took
all 84 from the same chunks over the same input path. Nine is `BridgeVM` plus
the space after it -- and that space is where packaged Notepad retitles its tab
(`*BridgeVM - Notepad`) and, between runs, comes back under a different frame
hwnd. Neither the 32-action cap nor the 28-character chunk size is anywhere
near the limit, so neither explains it.

A space-free payload settled it. `BridgeVM-p1ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789`
lands in full: the capture reads `Ln 1, Col 48` and `47 characters`, with the
tab, the `File Edit View` menu row and the body text all legible. The final run
is **3/3 classic and 3/3 packaged, six captures from six scenes, six confirmed
foregrounds and no failures**, against 1/3 and 0/3 at the start of the day.

The frame-time picture did not improve: the three CSVs carry 1, 3 and 2 data
rows. That clause still cannot be measured against these scenes.
