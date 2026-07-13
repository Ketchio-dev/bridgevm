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

A `BV_DRAW_WARMUP` variant (create+destroy a throwaway D3D10 device before the
real one, same process) also stays black — so it is not simply "first
D3D-device-open per boot" global init: a SECOND device in the same process
still fails. Only a second separate PROCESS passes. The distinguishing factor
is the process boundary (full `D3DKMTDestroyDevice`/VidMm process teardown of
process 1), not device-open count or draw count.

DECISIVE narrowing: the defect is strictly **per-process / per-first-D3D-device
after a guest boot** (and even per-first-process, since a second in-process
device does not help). A second process (fresh device + WDDM context, resource
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

## Root-cause narrowing — it is host-side, deterministic, guest-identical

Full per-resource lifecycle comparison of the failing (ctx 18) and passing
(ctx 19) draw-smoke runs *in the same boot* (`run3-truthful` trace):

- The two guest command sequences are **byte-identical except the four
  incrementing resource ids** (392–395 vs 396–399): same formats (RT bind=2,
  copy-dest bind=8, vertex fmt=177 24x1, staging-bounce bind=0x80000 1MB×1),
  same `RESOURCE_ATTACH_BACKING` ordering, same attach/submit/detach/unref.
- The host **masks no error for the app contexts**. The only masked
  `virgl_renderer_submit_cmd` errors in every boot are `ctx=6` (dwm) —
  "Illegal command buffer 786440" (that value decodes to a `DRAW_VBO` len=12
  header; dwm's own draws are rejected and are unrelated to the app). The
  app's `DRAW_VBO` submits are accepted.

Therefore the defect is **not guest-side** (identical commands), **not a
masked draw rejection** (app submits succeed), **not backing** (both sequences
attach identically), **not CGL binding** (falsified), **not per-context lazy
init** (3 in-process redraws fail), and **not first-device global init**
(warmup device fails). It is a **deterministic host virglrenderer/Mesa GL
execution difference between the first and second identical draw+copy+readback
sequence after a guest reset** — the first renders/reads black, every
subsequent one is pixel-exact. The BridgeVM host `reset()` destroys all
tracked virglrenderer contexts and resources on guest reboot, but the
process-global virglrenderer singleton and its ctx0 / GL driver state persist,
so "second sequence works" points at GL-driver/renderer state warmed by the
first real app draw+copy+readback after a reset.

Sharpest reframe (most valuable clue): `run3-truthful` was a **single boot with
no guest reboot** between process 1 (ctx 18, black) and process 2 (ctx 19,
correct). There was therefore **no host `reset()` between them** — the reset
angle is a red herring. What makes ctx 18 special is that it is the **first
context in the virglrenderer instance's lifetime to perform a *successful*
DRAW_VBO + RESOURCE_COPY_REGION + COPY_TRANSFER3D readback cycle**: dwm's ctx 6
issues `DRAW_VBO` but every one is rejected ("Illegal command buffer", EINVAL),
so no earlier context actually completes the draw→copy→readback path. This is
the classic signature of a **one-time, renderer-GLOBAL (not per-sub-context)
lazy GL-object initialization whose first user does not benefit from it** — the
object is created/bound during ctx 18's first copy/readback but ctx 18 has
already captured the pre-init state, while ctx 19 inherits the initialized
object. The lazy **blitter** was checked and ruled out: this 64×64 same-format
(`format 67 → 67`) `CopyResource` takes the `glCopyImageSubData` branch
(`feat_copy_image` is available on Apple GL 4.1), not `glBlitFramebuffer` and
not `vrend_renderer_blit_gl`/the blitter (`vrend_renderer.c:10953`). The
readback is a plain `glGetTexImage`; the draw uses the per-sub-context draw FBO.
None of these has an obvious renderer-global lazy object, so the exact
first-cycle-vs-second-cycle divergence now needs **runtime GL instrumentation**,
not static reading: boot with `VREND_DEBUG=copy_resource,tex,cmd` and diff the
per-GL-call path + a `glGetError` probe between the first (ctx 18) and second
(ctx 19) cycle; whichever GL call first diverges (or first clears an error) is
the culprit. (`vrend_destroy_context` was verified to clear
`current_ctx`/`current_hw_ctx` and force ctx0, so a dangling-current-context
use-after-free is ruled out; and there is no host `reset()` between the two
cycles, so this is purely a first-successful-cycle effect.)

Earlier framing (kept for the record): the first app **context** is poisoned
for its entire lifetime — `BV_DRAW_ITERS=3` (three full draw+copy+readback
cycles on the same device/context) fails all three, yet a fresh **second
context** (new process) works first try. So the corruption is bound to the
first post-reset app context, and only a brand-new context escapes it; repeating
work inside the poisoned context never recovers. This points at per-context GL
state established at the first app `CTX_CREATE`/sub-context setup after the host
`reset()` destroyed the previous boot's contexts (a likely-dangling
`vrend_state.current_ctx`/`current_hw_ctx` global, or first-context sub-context
GL objects), not at the draw/copy/readback commands themselves.

Next-session instrumentation (host, virglrenderer or venus_backend): for the
app context's `RESOURCE_COPY_REGION` and `COPY_TRANSFER3D`/`transfer_3d`
from-host, log `glCheckFramebufferStatus` of the scratch/blit FBO and a
first-pixel sample of the `glGetTexImage` result, and test whether a
`glFinish` (or `virgl_renderer_force_ctx_0` + flush) before the readback makes
the first sequence pass. gpt-5.6-sol's ranked hypotheses (KMD `VIOGPU_CTX_INIT`
attachment loss #1, VidMm first-device residency #2) were both weakened by the
byte-identical trace; the live evidence points squarely at host GL execution.

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
