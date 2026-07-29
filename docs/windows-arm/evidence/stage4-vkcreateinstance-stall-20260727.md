# stage4 stall: `vkCreateInstance` never returns (2026-07-27)

Run: `~/BridgeVM/runs/pw-000103` (pass 2, `--watchdog-ms 2400000`, 4 vCPU, 6 GiB).
Found by the boot-progress watchdog added in `013f883`.

## What the watchdog said

```text
probe: boot-progress watchdog stalled_for_ms=120000 exits_in_window=0
       total_exits=6125701 reboots=4 suspect=guest-not-running
```

Checked for a false positive before trusting it: the record sits at line 398 of
730 in `run.log`, immediately before `UEFI vars written back` — i.e. after real
progress, not during it. `reboots=4` matches the four `SYSTEM_RESET` lines.

## Where the guest actually stopped

`guest-logs/bvgpu-vulkan-probe.log` ends exactly here, with no matching end line:

```text
[vulkan-probe] direct_icd_load_end module_nonzero=True win32_error=0
[vulkan-probe] enumerate_instance_version_result=0 api_version=0x0040312d
[vulkan-probe] create_instance_begin
```

`bvgpu-vulkan-probe.ps1` calls `vkCreateInstance` through P/Invoke with no
timeout, so when the call does not return the whole firstboot chain hangs.
`viogpu3d-firstboot.log` corroborates: it stops mid-loader callstack, right
after `vkCreateInstance layer callstack setup to: <Application> || <Loader> ||
<Drivers>`.

Firstboot did reach stage4 (`firstboot_fresh=1`, `last_stage_observed=stage4`),
so this is **not** a boot failure. The 120 s RAMFB checkpoint shows a fully
composited Windows desktop — taskbar, Edge, terminal all rendering.

## The host GPU path is not the suspect

* Last command is `SUBMIT_3D` on ctx 21 answered `OK_NODATA`. The host replied;
  the guest then sent nothing.
* `venus: fence-poll watchdog ... outstanding_fences=0 suspect=idle-no-outstanding-fence`.
* Same signature as `p1-smoke2-103715`: host serviced everything, guest went
  quiet. Two different stall shapes, one shared conclusion.
* No Mesa/Venus ICD output reached `bvgpu-mesa-debug.log` — only unrelated
  Windows processes — so the ICD blocked before its first host round-trip.

## Two dead ends worth recording

**`RESOURCE_UNMAP_BLOB` is not the cause.** All 1828 of them failed
(`ERR_INVALID_PARAMETER`, 100%), against only 2 `RESOURCE_CREATE_BLOB`, and the
first unmap (seq 174) precedes the first create (seq 15378). That looks damning,
but `gpu-live-receipt-20260723.md:59` already records these as recoverable, and
they occur here while the desktop composites normally. Requests are well-formed
(all `request_len=32`, spec length), so the host is right to reject them; the
guest driver is asking to unmap blobs it never created.

**The `CTX_DESTROY` imbalance is not the cause either.** 89 creates vs 156
destroys, 86 of the destroys rejected, leaving a naive live count of -67. I
first suspected an even/odd `ctx_id` split from an 8-row sample; that was wrong
— creates include 24 even ids. All 89 `CTX_CREATE` succeeded. Platform reset
does clear `live_contexts` on reboot (`machine_assembly.rs:280` →
`virtio_gpu/device.rs:290` → `three_d.reset()`), so this is guest behaviour, not
leaked host state.

## What is genuinely wrong beyond the stall

`create3d/flush = 2.07` for the run, and never healthy in any window:

| seq window | create3d | flush | ratio |
|---|---|---|---|
| 0–2612 | 377 | 140 | 2.69 |
| 5224–7836 | 379 | 109 | 3.48 |
| 7836–10448 | 184 | 25 | 7.36 |
| 18284–20896 | 200 | 177 | 1.13 |

Healthy is ~0.02; ~1.0 already means presentation is rebuilding the swapchain
every frame. This run never got close, so the desktop screenshot showing a clean
image does not mean the presentation path was well.

## Follow-up run: the hang is intermittent, and it is not the main problem

`~/BridgeVM/runs/pw2-013847`, same media and flags, with the bounded probe from
`df34837` in place. `vkCreateInstance` **returned normally in 1126 ms**:

```text
[vulkan-probe] create_instance_result=0 instance_nonzero=True elapsed_ms=1126
[vulkan-probe] enumerate_physical_devices_result=0 count=1
[vulkan-probe] success
```

So the stage4 hang is intermittent, not deterministic, and the timeout did not
need to fire. (The log is genuinely from this run — sha256 `8c0695e3…` vs
`5ca51199…` for `pw-000103`. `firstboot_fresh=0` here because the image had
already completed firstboot, so the probe ran from the diagnostics path.)

That run then stalled somewhere else entirely: `reboots=1`, 165K exits, no
firstboot log, and the 15 s RAMFB checkpoint shows the **Windows boot spinner**
— the same early-boot shape as `p1-smoke2-103715`, not a stage4 failure.

| run | reboots | exits | stage reached | outcome |
|---|---|---|---|---|
| `p1-smoke-100443` | 2 | 0.5M | — | healthy |
| `p1-smoke2-103715` | 1 | 69K | none | early-boot stall |
| `pw-000103` | 4 | 6.13M | stage4 | `vkCreateInstance` hang |
| `pw2-013847` | 1 | 165K | none | early-boot stall |

**Priority correction (superseded below).** Early-boot stalls are 2 of the 3
failures; the stage4 hang has been seen once and did not reproduce.

## Correction 2026-07-27: the hang is dominant, not intermittent

A 5-boot slice of `p1gate-A1-20260727-064829` (gate aborted early, data kept):

| boot | stage4_pass | last_stage | stalls | reboots |
|---|---|---|---|---|
| 1 | 0 | stage4 | 1 | 4 |
| 2 | **1** | stage4 | 0 | 3 |
| 3 | 0 | stage1 | 1 | 0 |
| 4 | 0 | stage4 | 1 | 4 |
| 5 | 0 | stage4 | 1 | 4 |

1 pass in 5. Three of the four stalls end on the same line of
`bvgpu-vulkan-probe.log` — `create_instance_begin`, with no result — so
`vkCreateInstance` is the **leading** cause, not a one-off. The "intermittent"
verdict came from a single non-reproducing run and was wrong.

**But this gate proved nothing about the fix.** The timeout from `df34837`
never ran: the injector was built 2026-07-26 01:57, the fix landed 07-27 01:38,
and the injector is what carries the guest scripts. Byte-searching the images
settles it — `create_instance_timeout_ms` appears 0 times in the old injector
and once in a freshly built one. Every stall above is the pre-fix code.

`scripts/p1-boot-gate.sh` now refuses to start when any file in
`scripts/win-assets` is newer than the injector, because nothing in the gate
could previously catch this.


## Where inside `vkCreateInstance` (2026-07-27)

`bvgpu-mesa-debug.log` localises it further than the probe log can. All three
stalled boots (1, 4, 5) end on the **same** line:

```text
MESA-VIRTIO: debug: blob map escape ok handle=1073744256 offset=147456 ...
```

The passing boot 2 continues from exactly that point:

```text
MESA-VIRTIO: debug: blob map escape ok handle=1073744256 offset=147456 ...
MESA-VIRTIO: debug: renderer instance version 1.4.334      <- host reply
MESA-VIRTIO: debug: supports multi-plane wsi format modifiers: no
```

`renderer instance version` is a reply from the host renderer. The stalled runs
map the same two blobs and then never receive it.

The blob counts say the same thing:

| boot | outcome | CREATE_BLOB | MAP_BLOB |
|---|---|---|---|
| 2 | pass | 95 | 95 |
| 1 | stall | 2 | 2 |
| 4 | stall | 2 | 2 |
| 5 | stall | 2 | 2 |

Exactly two — the two in the log — then nothing.

**The host is not stuck.** After the last `MAP_BLOB` the stalled runs still
process ~5200 further commands (1158 `SUBMIT_3D`, 4 new `venus-win32`
contexts), and the fence-poll watchdog reports
`outstanding_fences=0 suspect=idle-no-outstanding-fence`. The GPU path keeps
serving other clients; only the probe process is blocked, waiting on a reply
that never comes.

So the failure is in the Venus ICD's first host round-trip during instance
creation, after the ring blobs are mapped. Nothing on the host side of the
virtio-gpu protocol shows a pending request to answer, which points at the
guest ICD or the ring handshake rather than the renderer.

Not yet ruled out: boot 2 created 8 `virgl-shadow-win32` contexts against 1 in
the stalled runs. That may be a cause or merely a consequence of getting
further; it is not evidence either way yet.


## Root cause narrowed to the first Venus ring command (2026-07-27, n=10)

Gate `p1gate-A1b-20260727-114738` (fresh injector, so the timeout fix was
live): **4 pass / 10**. Six failures, of which **five are this hang** —
boots 1, 4, 8, 9 report `create_instance_timeout_ms=120000` →
`errorlevel=13`, boot 10 stalled in stage1, boot 3 never ran firstboot.
One cause dominates the gate.

`RESOURCE_MAP_BLOB` separates them perfectly (n=8):

| boot | outcome | MAP_BLOB |
|---|---|---|
| 2, 6, 7 | pass | 83 |
| 5 | pass | 18 |
| 1, 4, 8, 9 | stall | **2** |

The Venus ring command stream separates them even more sharply. Filtering
`SUBMIT_3D` to the probe's own payload shape — `submit_first_command_id=251`,
`submit_dwords=6`:

| boot | outcome | cmd251 count | contexts |
|---|---|---|---|
| 2, 6 | pass | **1302** | 27 and 28 |
| 1, 4, 8, 9 | stall | **1** | 27 only |

The passing runs issue that command 1302 times across two contexts. The
stalled runs issue it **exactly once** and never again:

```text
boot 2 PASS                        boot 1 FAIL
seq 13730 SUBMIT_3D ctx27 cmd251   seq 13710 SUBMIT_3D ctx27 cmd251
seq 13731 SUBMIT_3D ctx27 cmd251   seq 13711 RESOURCE_DETACH_BACKING ctx0
seq 13732 SUBMIT_3D ctx27 cmd251   seq 13712 RESOURCE_DETACH_BACKING ctx0
seq 13733 SUBMIT_3D ctx27 cmd190   ...  ctx27 never appears again
```

The host answered that single command normally: `OK_NODATA` in 14.9 µs,
`response_header_valid: true`, `response_truncated: false`, fence 975 echoed
back in the response header. Other contexts keep being served for thousands of
commands afterwards.

So the guest sends the first command of the Venus ring handshake, receives a
well-formed reply, and then stops driving the ring. The failure is on the guest
side of that handshake, after the reply is delivered.

### Ruled out

- **Fence delivery.** The obvious theory — the guest waits on `fence_id=975`
  and the host never signals it — does not survive the passing runs, which
  also leave thousands of fenced submits undelivered (2954 of 3141 in boot 2).
  Undelivered fences are normal here, so this is not the discriminator.
- **Host being stuck.** `outstanding_fences=0`, ~5200 further commands served
  after the last probe command.
- **A different context colliding.** `ctx_id` 27 is reused across runs but
  carries different workloads (`cmd_id` 43/44, 1070–2869 dwords elsewhere), so
  identify the probe by payload shape, not by context id.

### Not yet known

Why the guest stops after one command. Candidates: the ring's doorbell/reply
slot is read once and then not re-armed, or the ICD blocks on a second
resource the two mapped blobs do not cover. Both need guest-side
instrumentation, not more host tracing — the host trace is now exhausted as an
evidence source for this bug.


## Exact instruction identified (2026-07-28, `VN_DEBUG` enabled)

`VN_DEBUG=init,result,log_ctx_info,no_abort` was added to the probe and the
injector rebuilt. Run `vn-repro-043918` boot 1 reproduced the stall with the
new tracing on (`vn_env is as below: debug = 0x33`).

The two runs are **identical for 12 lines** and diverge on the 13th:

```text
   stall (vn-repro boot 1)              pass (A1b boot 2)
 1 vn_env is as below                   (same)
 2 vn_renderer_create_virtgpu           (same)
 3 virtgpu_init                         (same)
 4 using virtio-win adapter             (same)
 5 connected to renderer                (same)
 6 wire format version 1                (same)
 7 vk xml version 1.4.343               (same)
 8 VK_EXT_command_serialization v1      (same)
 9 VK_MESA_venus_protocol v4            (same)
10 blob map escape ok handle=...064     (same)
11 D3DKMT submit context ready          (same)
12 blob map escape ok handle=...256     (same)
--                                      13 renderer instance version 1.4.334
                                        14 supports multi-plane wsi ...: no
                                        15 virtgpu_destroy
```

Line 13 comes from the ICD string `renderer instance version %d.%d.%d`, whose
neighbours are `failed to enumerate renderer instance version` and
`unsupported renderer instance version %d.%d`. It is the result of
**`vkEnumerateInstanceVersion` forwarded over the Venus ring** — the first
command the ring ever carries, and exactly the `cmd_id=251 / 6 dwords`
`SUBMIT_3D` that host tracing showed happening once and never again.

So the failing instruction is pinned: the guest issues the ring's first
command, the host replies `OK_NODATA` in 14.4 µs with a valid header, and the
guest never observes the reply. Everything before it — renderer connection,
protocol negotiation, both ring blob mappings, D3DKMT submit context — is
byte-for-byte identical between a passing and a stalling boot.

### Also ruled out this round

- **The ICD's own stall detectors.** `stuck in %s wait with iter at %d`,
  `aborting on ring fatal error`, `aborting on expired ring alive status`
  appear in the binary but in **none** of the stalled logs, so the hang is
  upstream of that retry loop.
- **`syncobj` creation**, which is the code path immediately after blob
  mapping in the binary's string order: no `virtgpu_sync_create` or
  `syncobj_*` message in either outcome.
- **`virgl_render_server socket disconnected`.** Present in every run — pass
  and fail — as the last line of `run.log`, i.e. normal teardown. It looked
  like a lead and is not one.

### Remaining question

Why the reply is not observed. The response is written to the ring's shared
memory (the second mapped blob, `offset=147456`), so the candidates are now
narrow: the guest reads a stale value from that mapping, or the notification
that the reply is ready is lost. Distinguishing them needs a host-side dump of
the ring memory at the moment of reply, which is the next step.


### The host response is byte-identical between pass and fail

Every field the trace records for that first `cmd251` submit matches:

```text
PASS: OK_NODATA len=24 planned=24 truncated=False header_valid=True
      flags=1 fenced=True ring_idx=0 ctx=27 writable_bytes=24
FAIL: OK_NODATA len=24 planned=24 truncated=False header_valid=True
      flags=1 fenced=True ring_idx=0 ctx=27 writable_bytes=24
```

Only `fence_id` differs (911 vs 908), which is just a counter.

The command is `fenced=True` yet **no** `fence_create` is emitted in either
outcome, so both take the `ChainCompletion::Immediate` path at
`virtio_gpu/virtqueue.rs:268` — the response is scattered into the writable
descriptor and the used ring is updated synchronously
(`virtqueue.rs:146-147`). No parked-fence delivery is involved, which
independently confirms the fence theory is dead.

`has_live_context(27)` also holds in both: the immediately preceding
`CTX_CREATE` for ctx 27 returns `OK_NODATA` with `debug_name=venus-win32` in
the passing run (seq 13721) and the stalling run (seq 12465) alike.

**Conclusion: the host side is exonerated for this command.** It receives a
well-formed request, writes a correct 24-byte response into the guest's
writable descriptor, updates the used ring, and raises the interrupt — the
same way in both outcomes. The divergence is entirely in what the guest does
after that.


## The guest stops notifying the queue (2026-07-28)

`vn-repro-043918` reproduced the hang in **3 of 4 boots** (boot 2 stalled
earlier, in stage1), each with `cmd251` issued exactly once — the same
signature as the A1b failures.

Counting `queue_notify` events after that first ring command:

| run | outcome | `queue_notify` after first `cmd251` |
|---|---|---|
| A1b boot 2 | pass | 3 |
| vn-repro boot 1 | stall | **0** |
| vn-repro boot 3 | stall | **0** |
| vn-repro boot 4 | stall | 1 |

In the stalling runs the last control-queue notification lands *before* the
ring's first command (index 12209 vs 12473 in boot 1) and the guest never
rings the doorbell again.

The host keeps working: 766 further commands are processed for other contexts
(DWM's ctx 21, ctx 0, ctx 7) without any new notification, via the polling
path. So the device is not wedged; the probe's thread simply never proceeds.

This narrows the remaining question to one thing: **was the completion
interrupt for that command actually raised?** The host code always raises it —
there is no `VIRTQ_USED_F_NO_NOTIFY` suppression anywhere in
`virtio_gpu/virtqueue.rs` or `interrupt.rs`, and the immediate path does
`write_used` then `mark_queue_interrupt` unconditionally
(`virtqueue.rs:146-147`) — but whether the MSI-X message was delivered is only
visible with `BRIDGEVM_TRACE_VENUS_START=1`, now reachable from the gate via
`--trace-venus-start`.

### Failure rate with `VN_DEBUG` on: not yet a signal

`vn-repro` failed 4/4 against A1b's 6/10. Tempting to read as "tracing changes
the timing", but at a 60% base rate four consecutive failures happen 13% of the
time. That is not evidence of anything and must not be treated as such without
more runs.


## MSI-X delivery is not the cause (2026-07-28)

`msix3-101159` ran three boots with `--trace-venus-start`, giving a passing
boot (3) and two stalling boots (1, 2) with the interrupt trace on.

| boot | outcome | MSI-X raised after the probe's `CTX_CREATE` | final counter |
|---|---|---|---|
| 3 | pass | 1 | n=14336 |
| 1 | stall | 0 | n=12288 |
| 2 | stall | 1 | n=12288 |

`msix ... held` — the suppressed-interrupt trace — is **0 in every run**,
passing and stalling alike. The passing run's higher final counter (14336 vs
12288) is a consequence of continuing to do work, not a cause.

A stalling boot raises the same number of interrupts around the probe's
context as a passing one, so **the interrupt path is exonerated**. Combined
with the earlier finding that the response bytes are identical, the host has
now been cleared on all three fronts: the response content, the used-ring
update, and the interrupt.

### Caveat on precision

`trace_sample` (`virtio_gpu/trace.rs:13`) keeps the first 64 events and then
every 1024th, so the counter resolves to ±1023. That is fine for "did
interrupts keep flowing" but cannot answer "was *this specific* completion
signalled". Answering that would need an unsampled trace keyed to the
command's `fence_id`.


## Retrying does not help — the outcome is fixed at boot (2026-07-28)

`retry2-142816`, six boots with up to three probe attempts each
(45 s timeout, only exit 13 retried):

| boot | attempt results |
|---|---|
| 1 | 13, 13, 13 |
| 2 | 13, 13, 255 |
| 3 | 13, 13, 13 |
| 4 | **0** |
| 5 | 13, 13, 13 |
| 6 | 13, 13, 13 |

**Second and third attempts: 0 successes out of 10.** The one passing boot
succeeded on its first attempt, so the retry contributed nothing to it either.

Under the null hypothesis that a retry is an independent draw at the measured
29% base rate, ten consecutive failures have probability 3.1%. That rejects
independence at the 5% level: **once a boot is going to fail, it fails every
time within that boot.** Whatever decides the outcome is fixed before the
probe runs — an initialisation order or a race resolved at boot — not a
transient the next attempt can dodge.

The mechanism is unchanged across attempts: every failed attempt stops at
`create_instance_begin`, the same place as before. (`boot 2`'s third attempt
exited 255 rather than 13 — PowerShell died before the guard's timer fired —
but its probe log ends at the same line.)

Gate rate with retry: 1/6. Without: 5/17. No improvement; the difference is
not significant at this sample size and should not be read as harm either.

**Consequence:** retry is removed. It costs up to two extra 45 s stalls per
failing boot and buys nothing, and leaving it in would misrepresent a
deterministic failure as a flaky one. The bounded timeout stays — it is what
turns a silent 40-minute hang into a reported `errorlevel=13`.


## Driver 120.43-fence-revert is no better than 120.45 (2026-07-28)

`drv43fast-224248`, eight boots on `download-120.43-fence-revert`, chosen
because its name points at fence behaviour and the stall lives in the ring.
The injector was byte-verified to carry that ICD: the two builds first differ
at offset 128, and the 120.43 signature appears once in the image while the
120.45 signature appears zero times.

| build | passes (fresh=1 only) | rate |
|---|---|---|
| 120.45-backing-only | 6/23 | 26% |
| 120.43-fence-revert | 2/6 | 33% |

Fisher exact p = 1.00. **No difference.** Failing boots stop at the same
`create_instance_begin`. Swapping driver builds is not the fix.

All six 120.4x builds share one mesa commit
(`cb531c440ff34a9c6334859dda0848132be49ec3`), so the ICD differences between
them are build-level, not source-level -- consistent with none of them moving
the needle.

### A second, distinct failure appeared

Two boots (2 and 3) came back `fresh=0 reboots=1`, and the final framebuffer
shows UEFI still at the TianoCore screen with `BdsDxe: failed to load Boot0003
"Windows Boot Manager" ... bootaa64.efi: Not Found` -- Windows never started,
so firstboot never ran and the guest logs are leftovers from the base image.
`firstboot_fresh` is what caught this; the stale logs otherwise look like a
clean pass, and one of them even contains a successful `create_instance` from
five days earlier.

Note that the same `BdsDxe: failed to load` line also appears in the serial
tail of boots that pass -- it is not by itself a failure signal. The
distinguishing evidence is the final framebuffer plus `fresh=0, reboots=1`.

Seen 2/8 here and 0/6 on 120.45. Too few to attribute to the driver build.
Tracked separately from the stage4 stall.


## VN_DEBUG=no_multi_ring does not help either (2026-07-29)

`nomultiring-014717`, eight boots on 120.45 with `no_multi_ring` appended to
VN_DEBUG. Chosen first among the ICD's behaviour switches because the symptom
is the guest going quiet after the ring's first command.

The switch was verified to reach the ICD rather than assumed:
`[stage4] vn_debug_extra=no_multi_ring` in firstboot's log and
`vn_debug=init,result,log_ctx_info,no_abort,no_multi_ring` in the probe's.

**0/8.** Against the 6/23 baseline that is Fisher p = 0.298 -- not enough to
claim it makes things worse, but no improvement whatsoever.

Failures also moved earlier: last_stage was stage1 once, stage2 once and
stage3 twice, where the previous two gates ended at stage4 every time. That
is consistent with the switch disturbing driver installation, but four events
cannot carry that conclusion on their own.

One boot ended with Windows up and a dialog box on screen, blocking stage4
from ever starting -- a failure shape not seen before.

**Reading the stage log needs care**: viogpu3d-firstboot.log accumulates
across the guest's internal reboots, so a `[failure] stage=stage2` line can
sit *above* later lines from a healthy stage2 in a subsequent generation.
Position in the file is not chronology within a boot.

### Where this leaves the stall

Three mitigations have now been measured and none moved it: process retry
(0/10 on second and third attempts), a different driver build (p = 1.00), and
an ICD behaviour switch (0/8). Combined with the host being fully exonerated,
the remaining options are to test the other switches
(`no_fence_feedback`, `no_cmd_batching`, `no_async_queue_submit`) or to accept
the stall as a known V1 limitation and spend the time on A2-A11 instead.


## The stall does not break the guest (2026-07-29)

Examining a `stage4 errorlevel=13` boot (`nomultiring-014717` boot 7) shows
Windows fully up: desktop, taskbar, wallpaper, indistinguishable from a boot
that passed.

Its virtio-gpu trace for that same run:

| event | count |
|---|---|
| command | 13546 |
| scanout_3d_flush | 37 |
| fence_create / complete / deliver | 65 / 65 / 65 |
| scanout_readback | 688 |

3D is live. Fences are created, completed and delivered in equal numbers.
`create3d/flush = 0` sits at the healthy end of the documented signature
(~0.02 healthy, ~1 presentation broken).

**So the stall blocks the diagnostic probe, not the guest.** stage4 exists to
assert that the Venus ICD can create an instance from a fresh boot; it is a
gate, not a feature. A user booting this image gets a working accelerated
desktop whether or not stage4 passed.

That reframes A1. As written it counts `stage4_pass=1`, which measures the
probe's success rate, and three separate mitigations have failed to move it.
Whether V1 should be gated on the probe or on the guest actually being usable
is an owner decision, and it is now the decision that matters most -- it is
the difference between V1 being blocked and V1 being shippable with a
documented limitation.


## The UEFI "Not Found" boot failure is not disk corruption (2026-07-29)

Seen 3 times in 23 boots (13%): Windows never starts, UEFI stays on the
TianoCore screen, and the serial tail reads `BdsDxe: failed to load Boot0003
"Windows Boot Manager" ... \\EFI\\Boot\\bootaa64.efi: Not Found`.

The injector pass is not at fault -- its final frame shows
`BVINJECT AGENT PLANT DONE` / `BVINJECT DONE`, i.e. it completed.

Mounting the affected disk afterwards shows everything UEFI claimed was
missing is present and well-formed:

| check | result |
|---|---|
| partition layout | EFI / MSR / Basic Data, intact |
| `\EFI\Boot\bootaa64.efi` | present, 3030944 bytes, starts `MZ` |
| `\EFI\Microsoft\Boot\BCD` | present, 32768 bytes, starts `regf` |

The NVRAM store does change during a run, but only by 5013 bytes out of 64
MiB, and `Boot0003` / `BootOrder` occur the same number of times as in the
pristine vars file. So this is not a wiped boot entry either.

**Cause not yet identified.** Ruled out: missing or truncated bootloader,
corrupt BCD, damaged partition table, failed injection, destroyed boot
variables. `firstboot_fresh=0` catches it reliably, so it never contaminates
a measurement -- which is how it was noticed at all, since the stale guest
logs it leaves behind otherwise read as a normal run.

Worth noting the same `BdsDxe: failed to load` line appears in the serial
tail of boots that pass, so the line alone means nothing; the final
framebuffer plus `fresh=0, reboots=1` is what identifies this.
