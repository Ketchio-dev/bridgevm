# A9 r44 3D-off display — failed physical diagnostic

Classification: one failed, nonpromoting physical diagnostic. The current A9
criterion, defect wording and product state come from
`capabilities/windows-hvf.json`; A9 remains OPEN and the product remains
ENGINEERING_PREVIEW.

## Sealed observation

Physical job `t17-35619a6d-display-click-pilot-r44` ran
`35619a6d61a760fbb4642cd76fc88a8cc3e56085`, the PR #278 head carrying the
frame-gated display click that followed [r43](a9-r43-display-click-20260930.md).
It was submitted at 06:09:53 UTC, before that head's hosted checks finished;
93 of its hosted runs later succeeded, including full project check
`36676621544`, and two pull-request runs were cancelled by the next push. The
app was a local exact-source Apple Development build, the Accessibility
preflight and console check passed, and the input manifest SHA-256 was
`beb49ca357e3183ba5c94d10a54fb0331856792aa8fc5df1d946db0433e03f92`.

The job ran on Mac17,9, macOS 27.0, from 06:10:19 to 06:22:03 UTC and failed.
Byte-identical receipts (SHA-256
`855595bd07e84d2f601ffcd5b02d2d96cfa22301cafab82586b010f652b49353`) passed the
strict read check; the lane result SHA-256 was
`1b53e7710c17727726233b5d5ae67fe373185bafd71550091beb6902ef333a75`. All 39
half-minute samples found the console unlocked, and the product app was
frontmost in all 23 samples taken while the lane ran.

Installation, Secure Boot provisioning and first READY each passed once, the
seventh consecutive first READY. The guest form wrote its ready marker. T17
opened the display and then failed after 15 seconds with `ui-element-missing`:
`guest display surface did not present a frame in the focused window`. No
click or keystroke was sent, and the run log carries no `POINTERINPUT` or
`TEXTINPUT` request.

The receipt outcome is `cleanup-failed`, not the lane failure, and this agent
session caused it. The tier's cleanup fence refuses while any process command
line names the lane work directory, and a log-reading loop run by this session
named that directory when the lane finished. The fence left the 30 GB
lane tree in place and fenced the queue. After the job, with no mounts and no
holders, the tree was removed with the tier's own identity-bound cleanup tool,
the job's source worktree was released as the worker does, and the fence was
cleared. The sealed receipt was not changed.

## Reading

The lane's runtime log records `virtio-gpu-pci: disabled`, as the supported
3D-off configuration requires, and its evidence directory held no
`display.fb`. That file is written only by the virtio-gpu device's framebuffer
sink, so in 3D-off the product display window has no frame source at all. The
surface therefore reported no frame and T17 correctly refused to click.
Windows was drawing: the retained ramfb checkpoint 15 seconds into the final
process shows the 800x600 Windows "Welcome" sign-in screen. The earlier 20/20
pointer campaign behind B4 used the virtio-gpu 3D driver path and says nothing
about 3D-off.

This corrects the [r43](a9-r43-display-click-20260930.md) reading. r43 ran the
same configuration, and its log also records `virtio-gpu-pci: disabled`. Its
lane tree is gone, so its evidence directory was not inspected, but with no
frame source its display could not present a frame. The r43 suggestion that
the click arrived before the first frame was wrong. The r43 record stays as
written.

## Follow-up

The runtime is gaining a ramfb export to `display.fb` for VMs without a
virtio-gpu device. Until that runs on hardware, the 3D-off product display
shows no guest screen and cannot take a pointer click. No journey stage after
first READY passed; A9 and A11 remain OPEN and product state stays
ENGINEERING_PREVIEW.
