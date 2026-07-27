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

## Next

The stall is inside `vkCreateInstance` in the guest ICD, before any host
round-trip. Host-side GPU instrumentation cannot localise it further. Options:
give the probe a bounded wait so stage4 reports instead of hanging, and get
Mesa/Venus ICD logging out of the loading process itself.
