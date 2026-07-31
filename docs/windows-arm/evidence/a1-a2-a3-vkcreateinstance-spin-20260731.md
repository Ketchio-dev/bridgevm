# A1/A2/A3 common wall: the vkCreateInstance spin

Status: investigation, open. This localizes the single point that now blocks
A1, A2 and A3, and identifies its mechanism: the guest and the host do not share
the Venus ring page. No criterion changes.

This document records current findings, not the order they were reached in.
Hypotheses that were tested and failed are kept under *Ruled out* so the work is
not repeated.

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

## How far `vkCreateInstance` gets

Mesa's own DBWIN log, captured fresh on the fixed driver
(`a3-dec3-20260731-120335`, provenance asserted below), shows the last thing it
manages before stalling:

```
MESA-VIRTIO: debug: vn_renderer_create_virtgpu
MESA-VIRTIO: debug: virtgpu_init
MESA-VIRTIO: debug: using virtio-win adapter
MESA-VIRTIO: debug: connected to renderer
MESA-VIRTIO: debug: wire format version 1
MESA-VIRTIO: debug: vk xml version 1.4.343
MESA-VIRTIO: debug: VK_EXT_command_serialization spec version 1
MESA-VIRTIO: debug: VK_MESA_venus_protocol spec version 4
MESA-VIRTIO: debug: blob map escape ok handle=1073744064 offset=0 ...
MESA-VIRTIO: debug: D3DKMT submit context ready context=0x40000900 ...
MESA-VIRTIO: debug: blob map escape ok handle=1073744256 offset=147456 ...
```

Then nothing. Renderer connection, protocol negotiation, the submit context and
both ring blob mappings all succeed; the stall is immediately after the second
ring shmem mapping.

The stage4 probe on a **working 3D desktop**
(`rethink-vioserial-activate-20260730-203138`) stops at the same call:

```
[vulkan-probe] enumerate_instance_version_result=0 api_version=0x0040312d
[vulkan-probe] create_instance_begin
[stage4] Vulkan probe errorlevel=13
```

`vkEnumerateInstanceVersion` succeeds; only `vkCreateInstance` hangs. A usable
Windows 3D desktop and a hanging `vkCreateInstance` coexist.

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

## It is intermittent, and the tell is `vkNotifyRingMESA`

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

This was originally read as the guest failing to wake a sleeping renderer. The
root-cause section below shows the opposite: Mesa never sends a notify because
it never observes the idle bit, because it is not reading the host's status
word at all.

## Root cause: the guest cannot see the ring's shared page

The renderer's own diagnostics had never been captured, because Venus does not
run in the VM process. `virgl_render_server` forks and execs
(`proxy/proxy_server.c:58-87`), and on macOS it then `posix_spawn`s a *worker*
per context (`server/render_worker.c:454-497`), so the process tree is three
deep:

```
34257  hvf_gic_boot_probe                                    <- VM
34258  └─ virgl_render_server --socket-fd 8                   <- server
34259     └─ virgl_render_server --worker-context-id 23 ...   <- Venus lives here
```

`vkr_log` reaches `render_log`, which is `vsyslog(LOG_DEBUG)` with `LOG_PERROR`
(`server/render_common.c:16-32`), so it does go to stderr and does reach
`run.log`. Instrumenting the ring thread produced the first renderer-internal
evidence of this investigation.

The ring is created and started normally, then the loop stops after one pass:

```
BVDIAG ring_start        ctx=23 ring=2132800336
BVDIAG ring_thread_enter ctx=23 cur=0 tail=0 idle_timeout=1000000
                         head_p=0x100534000 tail_p=0x100534040 status_p=0x100534080
BVDIAG ring_alive        ctx=23 n=0 cur=0 tail=0 head=0 status=0x4 ...
BVDIAG sleep_enter       ctx=23 cur=0 tail=0 status=0x5 pending_notify=0
```

Converting the renderer's `cnd_wait` to a one-second `cnd_timedwait` (semantics
unchanged: it still only exits on `pending_notify` or `!started`) shows what the
renderer sees for the whole stall — 151 consecutive samples, all identical:

```
BVDIAG sleeping ctx=23 cur=0 tail=0 status=0x5 pending_notify=0
                raw[0]=0 raw[16]=0 raw[32]=5
```

`raw[16]` is the tail word at offset 64. **The guest never writes it.** The
renderer is not sleeping through available work; there is no work.

### The page is shared, and only one direction of it works

Three independent checks, all in the same run:

1. The layout is valid and accepted: `res_id=194 res_size=147456 offset=0
   size=131268 headOff=0 tailOff=64 statusOff=128 bufOff=192 bufSize=131072`,
   matching Mesa's `vn_ring_get_layout` (`vn_ring.c:271-297`) exactly.
2. Ordering is correct: the GPU trace shows `RESOURCE_CREATE_BLOB` →
   `CTX_ATTACH_RESOURCE` → `RESOURCE_MAP_BLOB` (`OK_MAP_INFO`) → `SUBMIT_3D`.
   The mapping is established *before* the submit. `hv_vm_map` returns 0.
3. Host-to-host sharing is real. Stamping `0xBEEF0000` into the head slot from
   the **worker** process makes it visible in the **VM** process:

```
worker: raw[0]=3203334144            (= 0xbeef0000)
VM:     head=0xbeef0000 tail=0 status=0x5
```

So the worker and the VM share the page. What never appears in it is anything
written by the guest.

### The decisive test: the guest ignores a fatal ring

Mesa checks the status word on every submit and `abort()`s the process if the
fatal bit is set (`vn_ring.c`, `vn_ring_submit_internal`: `"vn_ring_submit abort
on fatal"`). Forcing that bit from the renderer after 20 seconds of silence:

```
BVDIAG forcing FATAL bit to test guest status reads
VM view: head=0xbeef0000 tail=0 status=0x7      <- FATAL|IDLE|ALIVE, host side
guest:   [vulkan-probe] create_instance_begin   <- still spinning, never aborts
```

The guest kept spinning. A guest that could read this page would have died
instantly. It did not, so **the guest's mapping of the ring blob does not refer
to the memory the host mapped.** The guest writes its tail, and the write lands
somewhere the host cannot see; it reads status, and reads something that is not
the host's status word.

This inverts the previous reading. `vkNotifyRingMESA` is absent not because
Mesa decided against sending it, but because Mesa is looking at a page that
never shows the idle bit — and the seqno it waits on is published into memory
the renderer never observes. It also explains the intermittency: whatever
decides the guest-side mapping is not deterministic across boots.

The defect is therefore in the guest-visible mapping of a HOST3D blob — the
`viogpu3d` "blob map escape" path and/or the guest physical address the VM
publishes for the shm window — not in Venus ring logic on either side.

## Ruled out

Recorded so the work is not repeated. Each was tested, not merely doubted.

**The renderer-side idle race.** Mesa notifies only when it already sees
`VK_RING_STATUS_IDLE_BIT_MESA` (`vn_ring.c:505-513`); the renderer sets that bit
only after `idle_timeout` and then parks in `cnd_wait`, which only a notify ends
(`vkr_ring.c:270-292`). A first submit inside that window would go unnotified
forever. Tested by patching `vkr_ring.c` to re-check the ring before sleeping,
rebuilding and installing both `libvirglrenderer.1.dylib` and
`virgl_render_server` and confirming the patched code in each. Three runs, all
hung, `190=0` in every one. **Disproven**; patch reverted. This also weakens the
causal reading of the notify counts: notify and seqno alternate 1:1 in the
passing run, which fits notifies being a symptom of a progressing ring as well
as their absence being the cause of a stuck one.

**The `RESOURCE_UNMAP_BLOB` rejections.** 332 rejections against 4 successes,
every one classified `never_created` by `--trace-venus-start` — which this
repository's own comment (`blob_host_mapping.rs:20-24`) calls a real
mapping-lifecycle bug. But a run that reaches a working 3D desktop
(`rethink-vioserial-activate-20260730-203138`) shows 1021 of the same
rejections, three times as many. A signal stronger in the passing case cannot
explain the failing one. Still a genuine defect; not this one.

**"The ring thread is never created."** Claimed from sampling 13 threads before
and during the spin. The measurement was invalid: Venus runs out of process
(`VIRGL_RENDERER_RENDER_SERVER`, `virglrenderer.c:932`), so `vkr_ring_thread`
lives in `virgl_render_server` — pid 79666 in that run, while pid 79665 was
sampled. Nothing followed from it.

**"The spin is steady-state `vn_relax`."** The ICD carries `vn_relax`'s strings,
but every profile warns after ~3.5 s (`vn_common.c:182-226`) and probes spun for
100 s with no warning at all. Whatever the thread is doing, it is not sitting in
that loop with a climbing `iter`.

**A stale-log artefact worth knowing about.** Reading
`C:\BridgeVM\bvgpu-mesa-debug.log` without deleting it first returns the
previous run's content. Two captures were initially misread this way; the
giveaway is an unchanged byte count and a `pid=` from the earlier run. Always
delete both logs and assert provenance by DriverStore hash and `captured_utc`.

## Where this leaves the diagnosis

Established:

- The stall is inside `vulkan_virtio.dll`, under `vkCreateInstance`, on the main
  thread. It is a spin, not a deadlock, and not a host poll outage.
- Renderer connection, protocol negotiation, submit context creation and both
  ring blob mappings all succeed first, and the host answers every command.
- The Venus ring is created and its thread started in the render-server worker.
  The renderer's view of the ring is coherent with the VM's view.
- **The guest's view is not.** The renderer never sees the guest's tail write,
  and the guest never reacts to a forced fatal bit in the status word. The two
  sides are not looking at the same memory.
- Contexts that never create a Venus ring keep working throughout, which is why
  a usable 3D desktop and a hanging `vkCreateInstance` coexist.
- The failure is intermittent: 1 of 6 runs completed `vkCreateInstance` in
  233 ms on the same image with identical flags.

Open:

- Which side of the guest mapping is wrong: the `viogpu3d` blob-map escape that
  hands Mesa a user VA, or the guest physical address the VM publishes for the
  shm window (BAR2 base + `shm_offset`).
- Why it is intermittent. A mapping that is simply mis-computed would fail
  every time; something boot-dependent (BAR assignment, allocation alignment,
  or a stale mapping left by an earlier context) must select between the two
  outcomes.

Next step: capture the guest-side mapping. Read back the ring page from inside
the guest at the user VA Mesa logs (`blob map escape ok ... user_va=...`) and
compare it against the host's stamped magic. That single comparison decides
between "the escape returns the wrong VA" and "the VA is right but the guest
physical mapping is not", and the two have different fixes.

## Method note

Two guest-side techniques were needed and are worth keeping:

- A workload started with `Start-Process` inherits the agent's stdout pipe and
  blocks the agent until it exits. `Invoke-CimMethod Win32_Process Create` gives
  the child no inherited handles, so the agent stays responsive while the
  workload hangs.
- Commands appended to the agent control file before the agent starts are
  skipped, so control lines must be appended only after `BVAGENT SERVICE start`.
