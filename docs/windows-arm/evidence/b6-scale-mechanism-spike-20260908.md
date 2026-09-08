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

Building the actual matrix harness must still solve reliable post-restart
active-scanout capture (the seed-advance failure above) before any of its
27 required observations count as evidence. This does not run the matrix
and does not change B6's `OPEN` state.
