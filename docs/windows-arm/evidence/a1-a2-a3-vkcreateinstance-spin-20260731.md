# A1/A2/A3 common wall: the vkCreateInstance spin, with a real stack

Status: investigation. This localizes the single point that now blocks A1, A2,
and A3. No criterion changes.

## Why this is one problem, not three

With the `0xD1` crash fixed
(`a3-attach-backing-0xd1-fix-20260731.md`), the D3D11 path runs far enough to
reach the same place A1's stage4 probe and A2's PPSSPP run already died:

| criterion | symptom | reported point |
| --------- | ------- | -------------- |
| A1 | stage4 probe times out after 45 s, errorlevel 13 | `vkCreateInstance` |
| A2 | PPSSPP waits forever during availability init | Venus bring-up started, then indefinite wait |
| A3 | workload never presents a frame | DXVK log stops before `vkCreateInstance` returns |

## The stack

Run `a3-spin-stack-20260731-094242`. The workload was launched detached through
`Win32_Process.Create`, and `cdb.exe` from the Windows SDK debuggers — extracted
in-guest from the chunked MSI — attached to the live process. Evidence:
`~/BridgeVM/runs/a3-spin-evidence/cdb-spin.txt`, sha256
`bad2420442f140a36f13b444c18ff59e49432b095a7bd4f74be89144c7dbab23`.

```
vulkan_virtio!vk_icdGetInstanceProcAddr+0x5099c
vulkan_virtio+0x33944
vulkan_virtio!vk_icdGetInstanceProcAddr+0x524f0
vulkan_virtio!vk_icdGetInstanceProcAddr+0xb0f8
vulkan_virtio!vk_icdGetInstanceProcAddr+0xacf4
vulkan_1!vkCreateInstance+0x4c4
dxgi!DXGIGetDebugInterface1+0xdfe4
d3d11!D3D11CoreCreateDevice+0x174
bridgevm_d3d11_present_fps!main+0x190
```

Every other thread in the process sits in
`ntdll!NtWaitForWorkViaWorkerFactory` — idle thread-pool workers. Only thread 0
is live, and it is inside the ICD below `vkCreateInstance`.

The loaded modules confirm there is no fallback path in play:

```
d3d11.dll        C:\BVSPIN\d3d11.dll                     (DXVK 3.0.2 aarch64)
dxgi.dll         C:\BVSPIN\dxgi.dll
vulkan-1.dll     C:\Windows\SYSTEM32\vulkan-1.dll
vulkan_virtio.dll C:\Windows\System32\DriverStore\FileRepository\viogpu3d.inf_arm64_6435ce2e01767d8f\vulkan_virtio.dll
```

## What the code at those addresses does

`vulkan_virtio.dll` has ImageBase `0x180000000` and exports
`vk_icdGetInstanceProcAddr` at RVA `0x55a70`, which maps the frames to:

| frame | RVA | VA |
| ----- | --- | -- |
| 0 | `0xa640c` | `0x1800a640c` |
| 1 | `0x33944` | `0x180033944` |
| 2 | `0xa7f60` | `0x1800a7f60` |
| 3 | `0x60b68` | `0x180060b68` |
| 4 | `0x60764` | `0x180060764` |

Frame 0 is not the loop that matters. It is an ordinary LL/SC retry inside an
atomic bit-clear:

```
1800a6400: ldaxr  w10, [x8]
1800a6404: and    w10, w10, w9
1800a6408: stlxr  w11, w10, [x8]
1800a640c: cbnz   w11, 0x1800a6400
```

The frame-1 caller is where the waiting is expressed. The function entered at
`0x1800338b4` reads a shared 32-bit slot at `[x21 + 0x418]` with acquire
semantics and compares it against a sequence value:

```
1800338e0: add    x23, x21, #0x418
1800338e4: ldar   w8, [x23]            ; acquire-load the shared slot
1800338e8: cbnz   w8, +0x3291c         ; already signalled -> done
1800338ec: ldar   w8, [x23]
1800338f0: cmp    w0, w8
1800338f4: b.eq   +0x3291c             ; reached the awaited value -> done
1800338fc: add    x0, x21, #0x3f0
180033900: bl     0x1801e5374          ; otherwise block/poll on the object
```

That is the Venus shared-memory FEEDBACK slot pattern: the guest publishes a
command, then waits for the host renderer to write a sequence number into the
shmem slot. `0x1801e5374` dispatches through a function pointer
(`ldr x8, [...]; blr x8`), so the concrete wait primitive is not resolvable
statically.

## Host side is not idle

The host instrumentation built for exactly this ambiguity
(`crates/bridgevm-hvf/src/venus_backend/poll_stall.rs`,
`poll_watchdog.rs`) stayed silent for the whole run. Neither
`fence-poll watchdog age_ms=...` nor a poll-stall record was emitted, so
`poll_fences` kept running and kept calling `virgl_renderer_context_poll` on
every live context.

Traffic also keeps flowing while the guest waits: the same run accumulated 577
`RESOURCE_ATTACH_BACKING`, 556 `SUBMIT_3D`, and 449 `RESOURCE_CREATE_3D`
commands. The transport is alive; the guest is waiting for one particular
answer that never satisfies its comparison.

The guest ring was set up immediately before the wait — `RESOURCE_CREATE_BLOB`
followed by `RESOURCE_MAP_BLOB` appear right before the command stream goes
quiet, which is the Venus ring being placed in host-visible shared memory.

## Ruled out: the UNMAP_BLOB rejections

The command trace shows `RESOURCE_UNMAP_BLOB` failing overwhelmingly:

```
RESOURCE_UNMAP_BLOB -> ERR_INVALID_PARAMETER   332
RESOURCE_UNMAP_BLOB -> OK_NODATA                 4
RESOURCE_MAP_BLOB   -> OK_MAP_INFO               4
```

Running with `--trace-venus-start` classifies every one of them:

```
venus-start: unmap_blob REJECT resource=23 reason=never_created
...
 118 reason=never_created
```

So the guest asks the host to unmap resources it never created as blobs, and
`blob_host_mapping.rs:20-24` calls exactly that class "a real mapping-lifecycle
bug or resource-id confusion". Both A2 and A3 show it at the same scale (119 and
118 rejections).

It is nevertheless **not** the cause of the spin. A run that reaches a working 3D
desktop shows the same rejections an order of magnitude more often:

| run | outcome | `UNMAP_BLOB` rejects | total commands |
| --- | ------- | -------------------- | -------------- |
| `rethink-vioserial-activate-20260730-203138` | working 3D desktop | 1021 (+8 OK) | 14031 |
| `attach-install3-20260731-074422` | driver install, healthy | 403 | 3461 |
| `a3-spin-stack-20260731-094242` | spun at `vkCreateInstance` | 332 | ~4500 |

A signal that appears more strongly in the passing case cannot explain the
failing one. It stays on the list as a real defect to fix later, but it is not
this wall.

## The last successful step, from Mesa's own log

> **Provenance warning.** The Mesa log quoted below came from
> `a3-mesa-log-20260731-103707`, which read `C:\BridgeVM\bvgpu-mesa-debug.log`
> **without deleting it first**. The file was stale: it carries
> `captured_utc=2026-07-31T00:34:13Z` and
> `viogpu3d.inf_arm64_44e90b7a44a1d335`, i.e. the previous day's run against the
> **shipped 120.41 driver**. The fixed driver on that image is
> `viogpu3d.inf_arm64_6435ce2e01767d8f`. So this sequence is a true record of
> where `vkCreateInstance` stalls on the *old* driver, and it agrees with the
> stack captured on the fixed driver, but it is not itself fixed-driver
> evidence. A fixed-driver Mesa capture is still outstanding.

Running the project's `bvgpu-vulkan-probe.ps1` with `BRIDGEVM_TRACE_VENUS_START`
and Mesa debug output captures the DBWIN log Mesa writes from inside
`vkCreateInstance`:

```
MESA-VIRTIO: debug: vn_renderer_create_virtgpu
MESA-VIRTIO: debug: virtgpu_init
MESA-VIRTIO: debug: using virtio-win adapter
MESA-VIRTIO: debug: connected to renderer
MESA-VIRTIO: debug: wire format version 1
MESA-VIRTIO: debug: vk xml version 1.4.343
MESA-VIRTIO: debug: VK_EXT_command_serialization spec version 1
MESA-VIRTIO: debug: VK_MESA_venus_protocol spec version 4
MESA-VIRTIO: debug: blob map escape ok handle=1073744064 offset=0 user_va=000001D1AAE10000 map_info=0x1
MESA-VIRTIO: debug: D3DKMT submit context ready context=0x40000900 command=1048576 allocations=1024 patches=1024
MESA-VIRTIO: debug: blob map escape ok handle=1073744256 offset=147456 user_va=000001D1ABF04000 map_info=0x1
```

Then nothing. Renderer connection, protocol negotiation, the submit context,
and both blob mappings all succeed. The stall is immediately after the second
ring shmem mapping.

The stage4 probe on a **working 3D desktop**
(`rethink-vioserial-activate-20260730-203138`) stops at the same call:

```
[vulkan-probe] enumerate_instance_version_result=0 api_version=0x0040312d
[vulkan-probe] create_instance_begin
[stage4] Vulkan probe errorlevel=13
```

`vkEnumerateInstanceVersion` returns successfully; only `vkCreateInstance`
hangs. A usable Windows 3D desktop and a hanging `vkCreateInstance` coexist.

## What the spin actually is

The shipped `vulkan_virtio.dll` contains these strings:

```
stuck in %s wait with iter at %d
aborting on ring fatal error at iter %d
aborting on expired ring alive status at iter %d
ring seqno / tls ring seqno / ring space
vn_ring_wait_space
vn_ring_submit abort on fatal
```

Those belong to `vn_relax()` in `src/virtio/vulkan/vn_common.c:248-290`, Mesa
Venus's backoff loop. It matches the observed behaviour exactly: it spins with
`thrd_yield()` for the first `1 << busy_wait_order` iterations, which is why one
thread burns a full core while the process makes no progress. The three wait
reasons it serves are `ring seqno`, `tls ring seqno`, and `ring space`.

So the disassembled acquire-load against `[x21 + 0x418]` is Mesa comparing a
ring seqno against the value the host renderer is expected to publish.

### The escalation that never fired — and why that matters

`vn_relax` is supposed to announce itself. `vn_common.c:182-226` gives the
profiles, and Mesa's own comments state the timings:

| reason | `busy_wait_order` | first warning |
| ------ | ----------------- | ------------- |
| `ring seqno` | 8 (256 yields) | iter 4096, ~3.5 s |
| `tls ring seqno`, `ring space`, `fence`, `semaphore`, `query` | 4 (16 yields) | iter 1024, ~3.5 s |

Every profile warns after roughly **3.5 seconds**, and aborts after ~895 s. Yet
probes that spun for 180 s and longer logged neither a warning nor an abort.

That is a contradiction, and it undercuts the simple reading of this loop:

- After `1 << busy_wait_order` iterations `vn_relax` stops spinning and calls
  `os_time_sleep` (`vn_common.c:293-294`). A thread parked in `os_time_sleep`
  does not burn a full core. But `a3-hang-diag-20260731-090047` measured thread
  3320 `state=Running` accumulating 45.7 → 186.0 s of CPU. A steady-state
  `vn_relax` cannot produce that.
- So either the code never advances past the yield phase because `iter` is being
  reset — i.e. an outer loop calls `vn_relax_init` repeatedly, so the counter
  never reaches `warn_order` — or the spinning thread is not in `vn_relax` at
  all and the ICD's `vn_relax` strings are a red herring.

The repeated-`vn_relax_init` hypothesis fits every observation: full-core spin,
no warning at any duration, and an acquire-load on a seqno slot. It also points
somewhere different from a plain stuck ring — at a caller retrying an operation
that keeps failing fast.

Until a capture shows `stuck in <reason> wait`, the wait reason is **not**
established. The earlier claim that this is straightforwardly "Mesa's vn_relax
ring wait" is therefore downgraded to a hypothesis with a known inconsistency.

## Fixed-driver capture: the last thing the guest ever sends

`a3-dec3-20260731-120335` is the first capture that is simultaneously fresh,
provenance-checked, and long enough to matter:

```
BV-ICD store=viogpu3d.inf_arm64_6435ce2e01767d8f          <- the fix, not 44e90b7a
BV-ICD sha256=A9356F9F837F82F288B3A8DB6F3BA4784C241316CA77AA0EA261D0B53CCA3429
BV-CLEAN mesa=False probe=False                           <- both logs deleted first
[vulkan-probe] captured_utc=2026-07-31T16:04:18.9592151Z  <- this run
BV-GO exit=13 elapsed_s=103.8                             <- spun for 100s
```

Mesa's fixed-driver log ends at exactly the same place as the old driver's, so
the `AttachBacking` fix changed nothing here, as expected.

The GPU trace names the last command the probe's context ever issues. Context 23
is the probe; it maps its two ring blobs, sends two `SUBMIT_3D`, and then goes
silent for the remaining 100 seconds while 291 further commands from other
contexts flow past it:

| idx | command | fence | response |
| --- | ------- | ----- | -------- |
| 1377 | `SUBMIT_3D` (172 B) | 673 | `OK_NODATA` |
| 1378 | `RESOURCE_CREATE_BLOB` | 674 | `OK_NODATA` |
| 1379 | `CTX_ATTACH_RESOURCE` | 675 | `OK_NODATA` |
| 1381 | **`SUBMIT_3D` (56 B)** | 676 | `OK_NODATA` |
| — | *(nothing further from ctx 23)* | | |

Decoding that final 24-byte Venus payload:

```
fb 00 00 00  00 00 00 00  30 f5 f5 c5  31 02 00 00  01 00 00 00  00 00 00 00
cmd   = 251 = VK_COMMAND_TYPE_vkSubmitVirtqueueSeqnoMESA_EXT
ring  = 0x00000231c5f5f530
seqno = 1
```

Command 251 is emitted from exactly one place in Mesa,
`vn_ring_submit_roundtrip()` (`vn_ring.c:783`). The guest publishes roundtrip
seqno **1** and then waits for the host to acknowledge it. That wait is the
spin. Everything before it — renderer connection, protocol negotiation, submit
context, both ring mappings, and the host's `OK_NODATA` for the submit itself —
succeeds.

So the guest is not stuck mid-render. It is stuck on the very first ring
roundtrip handshake of the instance.

## Fences are not the missing piece

The obvious next suspect is a dropped fence, and it is innocent:

```
fence_create: 64   fence_complete: 64   fence_deliver: 64
created but never completed: 0
```

Every fence in the run is created, completed, and delivered, including 676 for
the seqno submit. The host also acknowledged the submit itself with `OK_NODATA`.
What the guest is waiting for is therefore not the virtio fence but the seqno
value the host is expected to write back into the ring shmem.

This also explains the `vn_relax` contradiction. `vn_ring_wait_seqno()`
(`vn_ring.c:204-219`) keeps one `vn_relax_state` across the whole wait, so its
`iter` does climb and it *would* warn after ~3.5 s. A 100 s wait with no warning
means the guest is not sitting in that loop. Combined with the seqno-1
handshake, the likelier reading is a wait that never reaches
`vn_ring_wait_seqno` at all, or one that spins in the ring-status polling that
precedes it.

## The decisive contrast: ring users hang, non-ring users do not

While the probe's context 23 is frozen, context 21 keeps working perfectly in
the same run — 229 commands flow during the probe's silence, 57 of them from
ctx 21. Decoding what each context actually submits explains why:

| context | first Venus command id per submit | meaning | outcome |
| ------- | --------------------------------- | ------- | ------- |
| 23 (probe) | 188, then 251 | `vkCreateRingMESA`, then `vkSubmitVirtqueueSeqnoMESA` | **frozen after 251** |
| 21 (desktop) | 44 ×56, 43 ×8 | `vkGetEventStatus`, `vkDestroyEvent` | 64 submits, all fine |

The working context never creates a ring. It drives the desktop through plain
synchronous Venus calls. The probe creates a ring on its second submit
(`vkCreateRingMESA`, ring `0x231c5f5f530`), immediately publishes roundtrip
seqno 1 on it, and never returns.

That is the sharpest statement of the defect so far:

- Venus itself works on this host. Contexts that avoid the ring render the
  Windows desktop for the entire run.
- **The Venus ring path is what is broken.** Everything that needs
  `vkCreateInstance` needs a ring, which is why A1, A2 and A3 all die here while
  the desktop stays alive.

It also matches the host code. `virgl_renderer_context_poll` is the only thing
on macOS that retires renderer fences and writes the shmem slots Mesa polls
(`venus_start_trace_capset_cou.rs:600-610`), and there is no
`virgl_renderer_poll` anywhere in the tree. Whether that per-context poll also
services a Venus *ring* seqno is the open question, and it is now a narrow one.

## Retracted: the "ring thread is never created" measurement

An earlier revision of this document claimed the ring thread is never created,
based on sampling the host VM process before and during the spin and seeing 13
threads both times.

**That measurement was invalid.** Venus runs out of process. `init_flags()`
(`virgl_renderer_gl_context_mod.rs:283-292`) sets `VIRGL_RENDERER_RENDER_SERVER`
alongside `VIRGL_RENDERER_VENUS`, and `virglrenderer.c:932` only initialises the
Venus proxy renderer when that flag is present. The same run's log confirms a
separate process:

```
Jul 31 12:45:38  virgl_render_server[79666] <Debug>: socket disconnected
```

`vkr_ring_thread` lives in pid 79666, but the sample counted threads in pid
79665, the VM process. The measurement therefore says nothing about whether the
ring was created, and the conclusion drawn from it does not stand.

The consequences are worth stating plainly, because two follow-on inferences
were also wrong:

- The claim that `vkCreateRingMESA` bailed out before `vkr_ring_start` is
  unproven.
- The reasoning about `vkr_dispatch_vkCreateRingMESA`'s silent
  `VIRGL_RESOURCE_FD_SHM` guard rested on that claim and is likewise unproven.
  It is also the wrong code path to have been reading: with the render server in
  use, resources reach Venus through `proxy_context_get_blob`
  (`proxy/proxy_context.c:341,560`), not through the in-process route.

The absence of `vkr_log` output remains real but is now explained differently:
the renderer's stderr belongs to a different process and is not captured in
`run.log`. Capturing the render server's own output is the obvious next step,
and it was never done.

## It is intermittent, and the difference is `vkNotifyRingMESA`

With the render server correctly identified, the probe was run six times on the
same image with identical host flags. **One run succeeded:**

```
a3-rs-20260731-125330:   create_instance_result=0 instance_nonzero=True elapsed_ms=233
a3-rs2-20260731-131443:  hung
a3-rsN3 / N4 / N5:       hung
(one further run lost to an unrelated boot stall: PSCI reboot 1/8,
 stalled-between-boot-stages — the known A1 defect)
```

So `vkCreateInstance` is not deterministically broken. It completes in 233 ms
when it works. That reframes the whole problem: this is a race, not a missing
feature.

Comparing the ring context's Venus submit sequence between a passing and a
failing run makes the race concrete:

| run | ring context submits (in order) |
| --- | ------------------------------- |
| pass | `188, 251, 190, 251, 190, 251, 190, 251, 190, 251, 190, 251, 251, ...` |
| hang | `188, 251` |

Where:

- `188` = `vkCreateRingMESA`
- `251` = `vkSubmitVirtqueueSeqnoMESA`
- `190` = `vkNotifyRingMESA`

Counted across the whole run:

| command | pass | hang |
| ------- | ---: | ---: |
| `vkCreateRingMESA` (188) | 1 | 1 |
| `vkSubmitVirtqueueSeqnoMESA` (251) | 14 | **1** |
| `vkNotifyRingMESA` (190) | **7** | **0** |

Both runs create the ring. Both publish the first roundtrip seqno. In the
passing run the guest then issues `vkNotifyRingMESA` and the seqno/notify pair
alternates normally. In every failing run **no notify is ever sent**, and the
sequence dies at the first seqno.

That matches the renderer's structure exactly. `vkr_ring_thread` parks on
`cnd_wait(&ring->cond, &ring->mutex)` once the ring goes idle
(`vkr_ring.c:283-291`), and only a notify sets `pending_notify` and signals that
condition. A ring that is never notified has a servicing thread that never
wakes, so the seqno the guest is waiting on is never written — which is exactly
the observed silent, full-core spin with no `stuck in ...` warning.

### The race, in both codebases

The guest side decides whether to notify in `vn_ring_submit_internal`
(`vn_ring.c:505-513`):

```c
if (status & VK_RING_STATUS_IDLE_BIT_MESA) {
   ...
   return true;   /* only then is vkNotifyRingMESA sent */
}
return false;
```

Mesa sends a notify **only when it observes the idle bit already set**. If the
ring does not look idle, it submits and assumes the renderer's thread is awake
and will pick the work up.

The renderer side sets that bit only after an idle timeout has elapsed
(`vkr_ring.c:270-292`):

```c
while (ring->started) {
   bool wait = false;
   if (vkr_ring_now() >= last_submit + ring->idle_timeout) {
      ring->pending_notify = false;
      vkr_ring_set_status_bits(ring, VK_RING_STATUS_IDLE_BIT_MESA);
      wait = ring->buffer.cur == vkr_ring_load_tail(ring);
      ...
   }
   if (wait) {
      mtx_lock(&ring->mutex);
      while (ring->started && !ring->pending_notify)
         cnd_wait(&ring->cond, &ring->mutex);   /* sleeps until a notify */
```

That is the window. Between `vkCreateRingMESA` and the guest's first submit,
the renderer's fresh ring thread has not yet reached its idle timeout, so the
idle bit is still clear. The guest reads a not-idle ring, submits seqno 1, and
correctly-by-its-own-rules sends no notify. The renderer thread then crosses the
timeout, sets the idle bit, finds the buffer empty, and parks in `cnd_wait` —
which only `pending_notify` can end.

Both sides are individually reasonable; together they deadlock whenever the
guest's first submit lands in that window. When the timing falls the other way
the idle bit is already set, the notify goes out, and instance creation finishes
in 233 ms. One run in six is consistent with a window this narrow.

This is a hypothesis with strong support (the notify counts, the two code
paths, and the intermittency all agree), but it has not yet been confirmed by
observing the ring status word directly at the moment of the first submit.

## Where this leaves the diagnosis

Established:

- The stall is inside `vulkan_virtio.dll`, under `vkCreateInstance`, on the
  main thread, with the intended modules loaded.
- Renderer connection, protocol negotiation, submit context creation, and both
  ring blob mappings all succeed first.
- The spin is a spin, not a deadlock, and not a host poll outage.
- The host keeps polling and keeps retiring throughout.

Established (fixed driver, `a3-dec3-20260731-120335`):

- The last thing the guest ever sends is `vkSubmitVirtqueueSeqnoMESA` with
  roundtrip seqno 1, and the host answers `OK_NODATA`.
- Every virtio fence is created, completed, and delivered; none are dropped.
- The guest then issues nothing further for 100 s while other contexts continue.
- Contexts that never create a Venus ring keep working throughout the stall, so
  the defect is specific to the ring path rather than to Venus as a whole.

Not yet established:

- The failure is **intermittent**: 1 of 6 runs completed `vkCreateInstance` in
  233 ms on the same image with identical flags.
- The ring is created in both cases. The difference is `vkNotifyRingMESA`: 7
  notifies in the passing run, **zero** in every failing one.

Not yet established:

- Direct confirmation of the race. Mesa notifies only when it sees
  `VK_RING_STATUS_IDLE_BIT_MESA` already set (`vn_ring.c:505-513`), while the
  renderer sets that bit only after `idle_timeout` elapses and then parks in
  `cnd_wait` (`vkr_ring.c:270-292`). A first submit landing before the timeout
  therefore sends no notify and the ring thread never wakes. The counts, the
  code, and the intermittency all agree, but the status word has not yet been
  observed directly at the moment of the first submit.
- Why the render server's diagnostics never appear in the evidence. Its stderr
  is a separate process's and is not captured by `run.log`, which is why no
  `vkr_log` line has ever been seen — not because no error occurred.
- Whether the host publishes the awaited seqno at all, or publishes a value the
  guest does not accept.

The next discriminating step is to let the probe run past `warn_order` without
`no_abort` so Mesa prints `stuck in <reason> wait`, then correlate that reason
with what the host writes into the ring shmem — rather than changing poll
cadence or fence handling speculatively.

That capture was attempted and did not complete. Notes for the retry:

- `bvgpu-vulkan-probe.ps1` hardcodes `C:\BridgeVM\viogpu3d\` for the ICD, which
  does not exist on an image where the package was installed with `pnputil`.
  Mirror the DriverStore copy there first, as `a3-relax2` does.
- The probe's default `-VulkanProbeCreateTimeoutMs` kills the process before
  `vn_relax` reaches `warn_order`. A 20 s timeout cuts the capture off while the
  loader is still enumerating layers, well before Mesa logs anything.
- Reading `bvgpu-mesa-debug.log` without deleting it first returns the previous
  run's content. Two captures were initially misread this way — the giveaway is
  a stale `pid=` and an unchanged byte count.
- The whole sequence must fit inside one boot generation; the host watchdog
  ended the 10-minute attempt before its collection step ran.

## Method note

Two guest-side techniques were needed and are worth keeping:

- A workload started with `Start-Process` inherits the agent's stdout pipe and
  blocks the agent until it exits. `Invoke-CimMethod Win32_Process Create` gives
  the child no inherited handles, so the agent stays responsive while the
  workload hangs.
- Commands appended to the agent control file before the agent starts are
  skipped, so control lines must be appended only after `BVAGENT SERVICE start`.
