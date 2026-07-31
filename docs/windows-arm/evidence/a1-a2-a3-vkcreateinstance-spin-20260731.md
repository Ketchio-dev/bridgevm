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

## Where this leaves the diagnosis

Established:

- The stall is inside `vulkan_virtio.dll`, under `vkCreateInstance`, on the
  main thread, with the intended modules loaded.
- The waiting construct is an acquire-load comparison against a shared slot at
  `+0x418` of a context structure, i.e. a Venus FEEDBACK/seqno wait.
- The host keeps polling and keeps retiring; it is not a host poll outage.

Not yet established:

- Whether the host writes that slot at all, or writes it with a value the guest
  does not accept.
- Which command's completion the guest is waiting for.

The next discriminating step is to correlate the guest's awaited value with what
the host writes into the shmem FEEDBACK region, rather than to change poll
cadence or fence handling speculatively.

## Method note

Two guest-side techniques were needed and are worth keeping:

- A workload started with `Start-Process` inherits the agent's stdout pipe and
  blocks the agent until it exits. `Invoke-CimMethod Win32_Process Create` gives
  the child no inherited handles, so the agent stays responsive while the
  workload hangs.
- Commands appended to the agent control file before the agent starts are
  skipped, so control lines must be appended only after `BVAGENT SERVICE start`.
