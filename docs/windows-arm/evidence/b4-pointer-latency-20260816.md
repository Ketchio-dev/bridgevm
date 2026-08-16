# B4: what a host click actually costs, and why the first measurement said "absent" (2026-08-16)

## What B4 claimed

The capability registry recorded, for B4:

> HID delivery confirmed 1/1/1, but framebuffer checksums stayed at baseline
> through 1000ms, so visible reaction is >1000ms or absent.

No document contained that figure. This one does, and it also corrects it: the
guest does react, and two runs of the same command disagree about whether it
reacts at all.

## How it was measured

`scripts/measure-pointer-latency.sh` boots the installed Windows 11 Arm guest
with virtio-GPU 3D, waits for PPSSPP's deterministic Graphics Error dialog,
injects one absolute pointer move followed by a click at HID `22310x20800`, and
checksums the published framebuffer at fixed delays after the click.

That coordinate is the dialog's OK button. In an 800x600 frame,
`22310/32767*800 = 545` and `20800/32767*600 = 381`; the button in the captured
image is at approximately (544, 380). Dismissing a modal dialog is used rather
than clicking empty desktop because it guarantees a composited frame change.

Two runs exist from 2026-08-16, both under `~/BridgeVM/runs/`:

| run | delays sampled | reacted |
| --- | --- | --- |
| `pointer-latency-20260816-043026` | 5, 15, 30, 60, 120, 250, 500, 1000 ms | no |
| `pointer-latency-long-043700` | 50, 250, 1000, 2000, 3000, 5000, 8000, 12000, 20000, 30000 ms | yes |

## The run that reacted

Checkpoint checksums from `pointer-latency-long-043700/run.log`:

```
pointer-input-before         0x9299932141fd81cc
pointer-input-after          0x9299932141fd81cc
pointer-input-delay-50ms     0x9299932141fd81cc
pointer-input-delay-250ms    0x9299932141fd81cc
pointer-input-delay-1000ms   0x1b8a62c6d03051e9   <- first change
pointer-input-delay-2000ms   0xc0d93db8da0d3067
pointer-input-delay-3000ms   0xcd9324e91376cc73
pointer-input-delay-5000ms   0xcd9324e91376cc73
pointer-input-delay-8000ms   0xcd9324e91376cc73
pointer-input-delay-12000ms  0xcd9324e91376cc73
pointer-input-delay-20000ms  0xcd9324e91376cc73
pointer-input-delay-30000ms  0x37f3c810409fe45e
```

The captured frames show the whole causal chain rather than only a checksum
moving:

- `before`: the Graphics Error dialog with its OK button, menu bar reading
  `File / Emulation / Debugging / Game Settings / Help`.
- `delay-1000ms`: the dialog is gone, the mouse cursor is drawn at the click
  point with a busy indicator, and the menu bar has become
  `File / Emulation / Debug / Game settings / Help` — PPSSPP's post-dialog menu.
- `delay-3000ms`: PPSSPP has exited and the Windows desktop is showing.

So the click was received, the dialog was dismissed, and the application closed.
Visible reaction begins in `(250 ms, 1000 ms]` and the frame settles by 3000 ms.
The 30000 ms sample differs again because the desktop clock advanced.

## The run that did not react

`pointer-latency-20260816-043026` held `0x0531df34fe803143` at every delay from
5 ms to 1000 ms, and its 1000 ms frame still shows the dialog untouched. This is
the observation B4 was written from.

It is not a stalled host or a dead guest: the same run's periodic samples keep
changing afterwards (`ramfb-sample-30000ms` `0x3e444c31e885df94`,
`60000ms` `0x48f1be735668fcd0`, `90000ms` `0xceb36436a45fbe96`), so the guest was
compositing the whole time. It simply did not act on that click.

Host-side injection reported success in both runs, with identical counters:

```
fired=true attempted=true marker_seen=true
emitted_move_reports=1 emitted_button_reports=1 emitted_release_reports=1
rejected_count=0 empty_sequence_rejections=0 too_many_action_rejections=0
```

## What this establishes, and what it does not

Established:

- A host click does reach the guest and does produce a visible, correct,
  semantically meaningful reaction. B4's "or absent" is not the normal case.
- When it reacts, first visible change lands after 250 ms and by 1000 ms. That
  is slow for a pointer event and remains a real performance defect.
- `DEFAULT_VIRTIO_GPU_3D_SCANOUT_READBACK_MS = 16`
  (`crates/bridgevm-hvf/src/platform_virt/env_config.rs:6`) cannot account for a
  gap of this size, so readback cadence is not the explanation.

Not established:

- Why one of two runs ignored the click entirely. Both report identical HID
  emission counters, so the divergence is on the guest side of the USB boundary,
  after the host believes delivery succeeded.
- Where the 250-1000 ms is spent. This measurement brackets it; it does not
  attribute it between guest HID processing, application handling, DWM
  composition and scanout publication.

Sampling between 250 ms and 1000 ms would narrow the bracket, and repeating the
run enough times would give the ignore rate a denominator. One reacting run and
one ignoring run is not a rate.

## Why the earlier figure was wrong

The registry generalised from the first run alone, and that run happened to be
the one that did not react. Sampling stopped at 1000 ms, which is also the
boundary where reaction first appears in the run that did react, so even a
reacting guest could have produced "no change through 1000 ms".

B4 stays open with its measured text corrected to describe both outcomes.
