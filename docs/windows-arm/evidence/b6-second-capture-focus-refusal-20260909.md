# B6 second-capture failure: focus refusal the harness swallowed (2026-09-09)

Diagnostic reading of retained logs only. No live run was performed for this
document, no criterion changes, and B6 stays `OPEN`.

## What was claimed before

The 2026-09-08 handoff recorded "capturing a second, different window in the
same boot still fails" and hypothesised that `display.fb`/`display.fb.iosurface`
"tracks one active scanout resource and does not follow a switch to a genuinely
different window's resource once the first one goes away", with the follow-up
that the matrix might therefore need one boot per scene (54 capture boots
instead of 27). That hypothesis is not supported by the run it was drawn from.

## The export path was healthy for the whole boot

`~/BridgeVM/manifests/b6-matrix-harness-20260908/evidence/cell1-capture/run.log`
is a single boot covering three runs of two scenes. It contains exactly one

```
virtio-gpu: scanout IOSurface global id=417 (1600x900)
```

line. That message and the `<fb>.iosurface` descriptor write are both emitted
only when the surface id *changes*
(`crates/bridgevm-hvf/src/virtio_gpu/scanout_blit.rs:39-48`), so across all six
scene attempts the active surface never changed and the descriptor
`scripts/capture-active-iosurface.py` reads was never stale. The display also
kept updating: the run's checkpoint checksums differ at the 60 s, 90 s and
120 s samples.

The export mechanism therefore needs no fix for this failure, and the
one-boot-per-scene redesign is not justified by this evidence.

## What actually failed: SetForegroundWindow, ignored by the harness

| run | scene | hwnd | WINFOCUS | capture |
| --- | --- | --- | --- | --- |
| 1 | classic | 196668 | `-> OK WINFOCUS` | present |
| 1 | packaged | 262170 | `-> OK WINFOCUS` | absent |
| 2 | classic | 197218 | **`-> ERR WINFOCUS`** | absent |
| 2 | packaged | 197358 | `-> OK WINFOCUS` | absent |
| 3 | classic | 524318 | **`-> ERR WINFOCUS`** | absent |
| 3 | packaged | 328290 | `-> OK WINFOCUS` | absent |

The only capture that succeeded is the only classic scene whose focus request
succeeded. Both later classic scenes were refused the foreground, and the
harness proceeded anyway: `cell-capture.sh:208` and `:243` both send the focus
verb as

```sh
send "WINFOCUS $hwnd" "^BVAGENT WINFOCUS $hwnd -> OK WINFOCUS$" || true
```

so an `ERR` reply is discarded. The typed text that follows is the only thing
that invalidates the scanout -- already established live, since an idle desktop
plus a bare pointer move does not -- so keys delivered to a window that never
became foreground change nothing, no blit follows, and
`capture-active-iosurface.py`'s seed wait times out with
`RuntimeError: active IOSurface seed did not advance`. The error names the
symptom, not the cause.

## The packaged scene has a second, still-unexplained cause

Focus succeeded for all three packaged attempts and every packaged capture
still failed, so at least one further cause exists on that path. The leading
candidate is the first-run tip dismissal: the `POINTER click:547x303`
coordinate was estimated from a screenshot and has never been confirmed against
a real capture. That remains open.

## Not the cause: leaked windows or a blocked discard

`bv-windows-closure-discard.ps1` returns 13 from the second scene onward, but
its own output is `BVDISCARD hwnd=<n> pid=0 stopped=False`, and exit 13 is the
`$owner -eq 0` branch (`scripts/win-assets/bv-windows-closure-discard.ps1:17`):
the hwnd was already invalid when discard ran. The windows were gone, not
stuck, so this is a harness ordering wart rather than a leaked modal holding
the display.

A `Widgets` window with `0 852 0 0` bounds appears in `WINLIST` from run 2
onward and is one candidate for whatever owned the foreground when the focus
requests were refused. That is a guess; nothing in the retained logs confirms
it.

## Two notes for whoever builds the matrix tier

The one capture that did succeed
(`captures/classic-run1.ppm`, and the identical `captures/classic-run1/presented.ppm`)
renders the caption `*Untitled - Notepad` with its icon and the
minimise/maximise/close glyphs, the `File Edit Format View Help` menu row, the
typed `BridgeVM Probe run1 ABCDEFGHIJKLMNOPQRSTUVWXYZ abcdefghijklmnopqrstuvwxyz
0123456789` body line, and the `Ln 1, Col 85` status bar, all legible at
1600x900 with `LogPixels` 96. That is one scene, one run, one cell, read by eye
and not against a reviewed mask, so it promotes nothing; it does say the
caption-fix head renders this cell rather than blanking it.

The mask box cannot be placed from `WINLIST`. The harness reads `WINLIST`
first and only then sends `WINBOUNDS <hwnd> 50 60 700 500`
(`cell-capture.sh:207`), so the listed rect is the pre-move one -- 189,182
1186x611 for the run-1 window. The requested rect also is not the visible
frame: Windows 11's invisible resize border insets it. On this capture the
requested x span 50..750 renders as window interior 58..741 on row 200, about
seven pixels per side. Derive the region from the capture, or from the
requested rect with that border subtracted; a mask placed on either raw rect
would sample the wallpaper.

## What follows

Treat a failed `WINFOCUS` as a hard error: retry, confirm the foreground
actually changed, and refuse to capture a scene that never got focus, rather
than capturing whatever the screen happens to show. Do not raise the seed
timeout or drop the seed check to make these runs "pass" -- the seed check is
what caught this.
