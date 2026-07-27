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
