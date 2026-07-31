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

## Where this leaves the diagnosis

Established:

- The stall is inside `vulkan_virtio.dll`, under `vkCreateInstance`, on the
  main thread, with the intended modules loaded.
- Renderer connection, protocol negotiation, submit context creation, and both
  ring blob mappings all succeed first.
- The spin is a spin, not a deadlock, and not a host poll outage.
- The host keeps polling and keeps retiring throughout.

Not yet established:

- Whether the spinning thread is in `vn_relax` at all. The loop should warn
  after ~3.5 s and should stop spinning after its yield phase; neither happened,
  so either `iter` is being reset by a retrying caller or this is not `vn_relax`.
- Which wait reason applies, if it is `vn_relax`.
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
