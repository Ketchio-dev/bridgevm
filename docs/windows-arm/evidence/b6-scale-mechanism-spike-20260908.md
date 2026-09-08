# B6 scale mechanism observation: registry persistence only, effective DPI unmeasured (corrected 2026-09-08)

## Question

The B6 matrix needs three declared per-monitor display scales in addition to
the three already-proven resolutions. `bvgpu-apply-host-resolution.ps1` only
drives DEVMODE width/height; nothing in the repository changes the display
scale percentage. Public documentation for Windows 11 states the Settings
app applies a scale change instantly, but a registry-only change
(`HKCU\Control Panel\Desktop\LogPixels` /
`Win8DpiScaling`) needs a sign-out or restart to take effect. This spike
checks whether that registry-plus-restart path actually works on this exact
guest image, since the closure tier already forces guest restarts routinely
and could absorb one more.

## Method

Two new diagnostic-only guest assets, `bv-b6-read-logpixels.ps1` (prints
`BVLOGPIXELS value=<n>`) and `bv-b6-set-scale.ps1` (writes `LogPixels=144`
and `Win8DpiScaling=1`, i.e. 150%), both invoked with `-File` like every
other proven asset in this codebase — no inline `-Command` strings, which
this codebase's `cmd.exe /c` dispatch makes fragile to quote correctly. A
private, uncommitted harness variant (same F1/F2 steps as the proven
closure script) read the baseline value, wrote the new value, sent
`shutdown /r /f /t 0`, and waited for a second `BVAGENT SERVICE start` line
before reading the value back.

## Result

`BVLOGPIXELS value=absent` before (registry value unset; effective display
scale unmeasured), `reboot_recovered=true`, `BVLOGPIXELS value=144` after
the restart. The registry value was written, survived a full guest restart,
and read back correctly from the live guest's own registry query — not
inferred from documentation. This proves registry persistence only. It does
not prove effective display DPI: no window/monitor DPI was read, no visual
size comparison exists, and the absent-before value does not prove a 100%
display scale. The classic Notepad status-bar `100%` seen in other captures
is document zoom, not display scale, and is not used here as scale evidence.
The existing agent auto-restart/reconnect behavior, already exercised by the
driver-reboot tests, also covers a user-initiated restart without any new
harness work.

Both attempted visual captures (before and after) failed with
`RuntimeError: active IOSurface seed did not advance` from
`iosurface_capture.py`'s presented-frame validation. This is a capture-timing
bug in the ad hoc harness (a 3-second sleep before capture, evidently too
short or racing a stale scanout), not a defect in the scale mechanism; no
visual size comparison exists yet.

## Conclusion (narrowed 2026-09-08 — registry persistence only, not effective DPI)

The registry write-plus-restart path persists `LogPixels`/`Win8DpiScaling`
across a restart live: this narrows, but does not close, the second open
engineering unknown recorded in `PLAN.md`'s B6 matrix scoping section. What
is proven is the write/survive/read-back sequence; effective display DPI at
any scale remains unmeasured and no scale cell counts as declared until
effective window/monitor DPI is proven per run. Tab scene status is unchanged
(see `b6-tab-scene-spike-20260908.md`).

## Retraction 2026-09-08 — prior wording exceeded the observation

Any earlier reading of this spike as proving effective display DPI,
proving 100%/150% display scales, or closing the scale prerequisite is
retracted. The before/after captures both failed (see below), so no
effective-scale evidence exists yet. Failed-attempt history is preserved
verbatim below; only the conclusion drawn from it is narrowed.


## Follow-up: idle-desktop capture failure root-caused, 2026-09-08

The seed-advance failure above was chased further. Two more live attempts
isolate the cause: a direct capture 8s after Explorer settled still failed
with the same `RuntimeError`, and adding `POINTER move:400x300` immediately
before the capture (matching the exact test-fixture grammar in
`live_input.rs`) also failed. `run.log` shows no `kind=pointer_move` line
for the single move (`live_input.rs` only prints that line every 1024th
accepted move by design), and no `RESOURCE_FLUSH`/new frame followed it in
`virtio-gpu.jsonl`.

**Retraction 2026-09-08 — what the pointer/cursor sentences below do not prove:**
the missing log line plus the failed capture do not establish that the
pointer command was accepted, and they do not establish a hardware cursor
plane. Cursor-plane composition and pointer delivery remain unproven. What
is observed is only the correlation below, stated as a harness usage
constraint, not as a proven mechanism.

**Observed correlation: a bare pointer move did not invalidate the scanout
in these attempts.** Every capture that has ever succeeded in this project
followed a real content-changing action -- typed text (F4), a resize (F2),
or `WINCLOSE` (F3) -- never an idle desktop or a mouse move alone. A
possible explanation is a separately composited cursor plane that does not
touch the exported IOSurface-backed scanout resource, but that mechanism is
hypothesis, not established fact.

**Implication for the B6 matrix harness:** each of the 9 cells (3 runs each,
27 runs total, not 81) must capture immediately after a genuine
content-changing action already proven to produce a fresh frame (e.g.,
typing into the scene, or the scene-launch action itself while it is still
rendering), not after an idle settle-then-nudge pattern. This is not a new
capture-tool bug to fix; it is a usage constraint the existing tool always
had, now confirmed live rather than assumed.
Building the actual matrix harness must apply this constraint at every
capture point; this does not run the matrix and does not change B6's
`OPEN` state.
