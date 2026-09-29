# A9 r35 first-boot observation — failed physical diagnostic

Classification: one failed, nonpromoting physical diagnostic. The current A9
criterion, defect wording and product state come from
`capabilities/windows-hvf.json`; A9 remains OPEN and the product remains
ENGINEERING_PREVIEW. No clean-machine ISO or import journey is proven here.

## Sealed observation

The exact-main source was `52be583abc6dc09b5e5310f4d5d06de14e67ce05`,
which includes the bounded AX capture from `33c016f0` (PR #267). Same-SHA
hosted CI `36520747723`, Security and quality `36520747925` and the full
project check `36520784904` succeeded. The app was a local exact-source
Apple Development build; the helper's designated requirement names the
identifier and certificate, not a code hash, and the LaunchServices
Accessibility preflight passed before submission. The input manifest SHA-256
was `68fed5e47448e4b008a55da6eaf333d0ee335a52a0bbbb7c7295c6a6e258ee22`.

Physical job `t17-52be583a-dashboard-ax-diagnostic-r35` ran on Mac17,9,
macOS 27.0, from 20:38:17 to 20:55:57 UTC and ended failed with exit status
1. Its public and private strict T17 receipts were byte-identical at SHA-256
`8786e07c6387e75965cec17be5126979d3182f249e9b4f7ee038c31f1a77cb3c`; both
official receipt validators passed against the exact commit, and so did
`bridgevm-live receipt`. The result is failure: `first-boot-failed`, one run,
zero first READY passes, `criterion_pass=false` and
`capability_promotion=false`. Worker cleanup was verified.

The authenticated lane result SHA-256 was
`d69f954ab5194d383b520200bc79f2400d6c9cf26b535f54402891ac35767bcb`. VM
creation, source preparation, Windows installation and Secure Boot
provisioning each passed once. Unlike r34, the lane reached the runtime view
and pressed Start: the runtime log exists and the first-READY waiter ran. The
r34 `windows=0` lookup failure did not recur, so the new AX capture had no
dashboard failure to record. The lane then failed `guest-evidence-missing`:
no `BVAGENT READY` or proactive PONG within the unchanged 600-second window,
with `ready=0`, `service_start=0`, `system_reset=3` and `system_off=0`.
`host_stop` was `complete` (generation 2, report complete). This is the first
live run in which the terminal-footer correction `9c4e2858` (first included
in r34, which stopped earlier) produced a complete host-stop record; r33,
which predates it, recorded an incomplete one.

## Retained private packet

The private diagnostic packet stayed outside git. It retained the complete
455,234-byte runtime log (SHA-256
`7331e97919915dafbe12eb9f324dd4b965fd9bfa1ab8ed45a3047d636f1413aa`) and,
for the first time on this failure class, the final 800x600 ramfb frame (raw
SHA-256 `99934183adf77f384f466e98fd32d99e9a0960e3d6becb1b7f5dca820cb225d7`,
PPM SHA-256 `6b1502e53e02ca6ea8a3d36332857f25382a849196ecb5b4148037f2e79837a2`).
`display.fb` was absent because virtio-gpu is disabled in the 3D-off runtime.

Read directly from that packet, with its carriage-return line separators
normalized:

- The runtime ran three process generations. The first two ended by an
  orderly guest PSCI `SYSTEM_RESET` (NVMe I/O queues deleted and bus master
  cleared first) and process recreation with exit 42; the third ended by the
  host diagnostic stop. The lane detail `system_reset=3` counts the
  per-generation reboot-limit banner, so it records three generations and two
  resets, not three resets.
- Host counter deltas between generation ends put the second generation at
  about 418 seconds and the third at about 87 seconds when the unchanged
  600-second window, which starts at the Start press, expired.
- The zeroed agent-console and virtio-net state printed at the end of the
  first two generations is the state left by the guest's own device reset; the
  NAT counters, which that reset does not clear, show guest network traffic in
  both. Those end-of-generation lines are therefore not evidence about driver
  binding.
- In the third generation the agent console reached `status=0xf` with port 1
  ready and host-open, but the guest never opened port 1 and sent only three
  control messages; `agent_confirmed=false`. Virtio-net was also at
  `status=0xf` and had received about 43 MB.
- At the diagnostic stop all four vCPUs were in the kernel WFI idle loop with
  the probe's stall verdict false.
- The final frame shows the Windows setup screen "This might take a few
  minutes. Don't turn off your PC".

So the guest kernel's console driver was running, but no guest process opened
the agent port before the window expired. These are observations, not a cause:
they do not establish whether setup would have finished, whether first logon
ran, or whether the agent provisioner ran. No Windows setup log, provisioning
log or intermediate frame after the 15-second checkpoint was retained. The
final disk and variables hashes were recorded only in the private lane result;
the public receipt reports them as `absent`, and the cleaned files cannot be
rehashed.

## Limit

This is one pilot lane. It adds no journey pass, does not exercise T19 import,
a clean machine or Developer ID signing, and does not change the 600-second
bound or any pass condition. A9 and A11 remain OPEN, product state stays
ENGINEERING_PREVIEW, and 3D remains outside the release path.
