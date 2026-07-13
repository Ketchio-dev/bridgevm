# Windows D3D10 submission wall — resolution and the real wall (2026-07-14)

## The old wall was a trace artifact

`docs/hvf-3d-current-wall-20260713.md` anchored the Windows wall on "the owned
context emits no non-empty `SUBMIT_3D`". That statement was FALSE, produced by
the GPU trace sampler: successful `SUBMIT_3D` commands were sampled
(first 64, then every 1024th), and the Windows KMD's 60 Hz vsync no-op
heartbeat exhausted the always-record budget within the first minute of every
boot. Every real application command buffer submitted after that vanished from
the JSONL trace while executing normally. Fixed in `679336e` (empty submits
stay sampled; nonempty submits are always recorded) with a regression test.

With truthful tracing, a single boot of the instrumented test disk shows dwm
and application contexts streaming nonempty submissions continuously — the
desktop actually is host-rendered through VirGL, and the whole
UMD → D3DKMTRender → KMD → virtio `SUBMIT_3D` pipeline works.

## Instrumented-UMD evidence (guest side fully proven)

The submit-trace Mesa UMD package (BV-D3D10-ENTRY / BV-VIRGL-SUBMIT via
OutputDebugString, captured by `bridgevm-debug-runner.exe`) was built in the
Windows ARM64 builder VM (UMD-only mode, prebuilt pinned KMD), finalized,
injected into the test disk (replacing oem4.inf), and run live:

- Every owned draw run submits 4 batches (5548/4164/4676/7552 bytes) and all
  four `pfnRenderCb` calls return `STATUS_SUCCESS` with sane
  cdw/CommandLength/allocation-list values.
- All four batches arrive at the host intact (sizes, fences, and — with
  `--gpu-trace-submit-prefix` full payload capture — bytes).
- Decoded batch structure: `TRANSFER3D`(vertex upload) → pipeline setup →
  `CLEAR_SURFACE` → `DRAW_VBO(12)` → `RESOURCE_COPY_REGION`(target→staging),
  then `COPY_TRANSFER3D`(staging readback via bounce), then teardown batches.

## The real wall: first-draw-per-boot renders nothing

Reproducible on every boot of the test disk:

- The FIRST `bridgevm-d3d10-draw-smoke.exe` process after a guest boot reads
  back black (`center=000000ff`, 0 magenta pixels).
- The SECOND (and later) identical process in the same boot reads back a
  perfect fullscreen magenta triangle (4096/4096 pixels): `BV-D3D10-DRAW-PASS`.

This is the FIRST live proof that the owned D3D10 draw executes end-to-end on
the host renderer (shader compile in guest, draw on host GL via CGL, readback
verified pixel-exact).

Discriminating experiments (all live, preserved under
`~/BridgeVM/viogpu3d-submit-trace-draw-20260713-v1/run*`):

- Full-payload diff of the failing vs passing run: byte-identical command
  streams except the 4 incrementing resource ids. Viewport/draw/readback
  parameters identical.
- No vrend/GL errors for either run (`VREND_DEBUG=err,shader` and stderr are
  clean; the only vrend error of the whole boot is an unrelated boot-time
  `context 6 failed to dispatch DRAW_VBO: 22`).
- A D3D10 EVENT query completes (S_OK) in both runs.
- 3-minute idle before the first run: still black — ordinal, not elapsed time.
- `BV_DRAW_NOVB=1` variant (SV_VertexID fullscreen triangle, no vertex buffer,
  no input layout, no vertex transfer): first run still black — the vertex
  upload path is exonerated.
- Guest reboot with the host probe process kept alive: first run black again.
  (Note: a guest reboot also resets the virglrenderer instance, so this does
  not by itself exonerate host-side state; vrend and the guest reset together.)

Two hypotheses tested and FALSIFIED:

- Host CGL stale-context binding (vrend `current_hw_ctx` is process-global but
  CGL binding is thread-local; a pacing-thread readback could move the bind).
  Added an unconditional Apple rebind in `vrend_finish_context_switch` +
  `vrend_hw_switch_context_with_sub` with a `BV-CGL-REBIND` diagnostic.
  Result: 0 corrections, first run still black. Reverted (0 benefit, hot-path
  cost). The global bookkeeping was never wrong on this workload.
- Per-first-draw-of-a-context lazy init (shader compile / VAO / FBO). Ran the
  full draw+CopyResource+readback THREE times inside one process on the same
  device/context (`BV_DRAW_ITERS=3`): all three black. Not warm-up, not lazy
  per-context state.

DECISIVE narrowing: the defect is strictly **per-process / per-first-D3D-device
after a guest boot**. A second process (fresh device + WDDM context, resource
ids that increment past the first) reads back correct pixels; the first process
never does, no matter how many times it redraws. Since the host virglrenderer
instance and its GL contexts persist across guest PSCI reboots yet the failure
recurs every boot, and process 2 runs the identical host path successfully, the
cause is **guest-side (VidMm/dxgkrnl/KMD) first-device-after-boot state**, not a
host renderer lazy init. Prime suspects: a DEFAULT-pool render-target's guest
backing (AttachBacking MDL page list, unfenced `QueueBuffer`) not being
host-visible before the fenced `SUBMIT_3D` that draws into it on the cold path;
or a one-time per-adapter/first-device initialization the first process triggers
but does not itself benefit from. Under investigation (gpt-5.6-sol KMD/host
analysis task-mrjnaq1v).

## Infrastructure fixed/added along the way (each committed with tests)

- `5e5f711` pinned-commit shallow fetch with retries for guest build kits.
- `6cbd80e` NAT: 256-frame RX bursts per poll (was 1); honest per-flow
  activity stamping (mid-download resets + starvation fixed; verified live
  with a 130MB pack through the NAT in ~1 minute).
- `e142f59` NAT idle eviction re-based on wall-clock milliseconds.
- `cd0219f` UMD-only rebuild mode (`-DriverSysPath`) reusing the pinned CI
  KMD, bypassing the missing WDK VS toolsets (MSB8020).
- `1c71bbe`/`bc05efb` Mesa python deps pinned (packaging, mako, PyYAML).
- `c78d432` winflexbison carried beside the kit for Mesa's flex dependency.
- `0a9a01d` meson `/MACHINE:arm64` canonicalization for clang-cl arm64
  triples (ported from the proven CI fix).
- `679336e` trace sampler fix (see above).
- `7a80b5e` `--gpu-trace-submit-prefix` full-payload capture.

## Next work

1. Root-cause the first-draw-per-boot black readback (host CGL/vrend lazy
   init is the prime suspect; instrument attach-backing page lists and the
   copy-transfer write path if code inspection stalls).
2. Re-run the winsat/perf gates against truthful tracing (the old
   "3 nonempty submits" numbers are meaningless now).
3. The builder VM (windows-arm64-submit-trace-builder-v1.raw) now builds the
   instrumented UMD end-to-end in ~25 minutes; keep using it for Mesa-side
   experiments.
