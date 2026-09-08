# B6 scale mechanism spike: registry + reboot proven live (2026-09-08)

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

`BVLOGPIXELS value=absent` before (default 96/100%, unset), `reboot_recovered=true`,
`BVLOGPIXELS value=144` after the restart. The value was written, survived a
full guest restart, and read back correctly from the live guest's own
registry query — not inferred from documentation. The existing agent
auto-restart/reconnect behavior, already exercised by the driver-reboot
tests, also covers a user-initiated scale-change restart without any new
harness work.

Both attempted visual captures (before and after) failed with
`RuntimeError: active IOSurface seed did not advance` from
`iosurface_capture.py`'s presented-frame validation. This is a capture-timing
bug in the ad hoc harness (a 3-second sleep before capture, evidently too
short or racing a stale scanout), not a defect in the scale mechanism; no
visual size comparison exists yet.

## Conclusion

The registry-plus-restart mechanism is confirmed live: this closes the
second and final open engineering unknown recorded in `PLAN.md`'s B6 matrix
scoping section. Both spikes needed before building the declared 3x3x3
harness are done:
- tab scene: File Explorer's Home tab (see
  `b6-tab-scene-spike-20260908.md`);
- scale change: `bv-b6-set-scale.ps1` plus a guest restart (this document).


## Follow-up: idle-desktop capture failure root-caused, 2026-09-08

The seed-advance failure above was chased further. Two more live attempts
isolate the cause: a direct capture 8s after Explorer settled still failed
with the same `RuntimeError`, and adding `POINTER move:400x300` immediately
before the capture (matching the exact test-fixture grammar in
`live_input.rs`) also failed. `run.log` confirms the pointer command was
silently accepted -- `live_input.rs` only prints `kind=pointer_move` every
1024th accepted move by design, so a single move produces no log line, but
the underlying `queue_xhci_pointer_input_actions_with_mem` call still ran.
No `RESOURCE_FLUSH`/new frame followed it in `virtio-gpu.jsonl`.

**Root cause: a bare pointer move does not invalidate the scanout.** Every
capture that has ever succeeded in this project followed a real
content-changing action -- typed text (F4), a resize (F2), or `WINCLOSE`
(F3) -- never an idle desktop or a mouse move alone. The guest's hardware
cursor is evidently composited on a separate plane that does not touch the
exported IOSurface-backed scanout resource, so moving it alone never
produces a new frame to capture.

**Implication for the B6 matrix harness:** each of the 27 cells must
capture immediately after a genuine content-changing action already
proven to produce a fresh frame (e.g., typing into the scene, or the
scene-launch action itself while it is still rendering), not after an
idle settle-then-nudge pattern. This is not a new capture-tool bug to fix;
it is a usage constraint the existing tool always had, now confirmed live
rather than assumed.
Building the actual matrix harness must apply this constraint at every
capture point; this does not run the matrix and does not change B6's
`OPEN` state.
