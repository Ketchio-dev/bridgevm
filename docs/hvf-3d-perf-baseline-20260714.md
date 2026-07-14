# Windows 3D — first real performance baseline (2026-07-14)

## Why WinSAT numbers are not performance numbers here

`winsat d3d` completes exit-0 on BridgeVM but reports the identical
42.00 F/s for every subtest (Batch/Alpha/Tex/ALU on DX9 and DX10 classes,
Geometry tiers, CBuffer). A figure that does not move across workloads of
wildly different cost is measuring the presentation cadence, not the
renderer. WinSAT remains useful only as a does-real-D3D-work-run gate.

## The owned benchmark

`scripts/win-tests/bridgevm-d3d10-bench.c` (built by
`scripts/build-hvf-windows-d3d10-smoke.sh`, run via the agent share like the
smokes) renders present-free: offscreen 1280x800 R8G8B8A8 target, per frame
one clear plus `BV_BENCH_DRAWS` (default 100) instanced quad draws with a
per-draw `UpdateSubresource` constant-buffer upload — deliberately
exercising the guest→host buffer-upload path that the first-draw fix added
a host `glFlush` to — and a `Flush` per frame. GPU completion is fenced
with a `D3D10_QUERY_EVENT` poll (Map alone does NOT reliably block on this
stack) before the clock is read, then the result is copy/mapped and
pixel-verified. Knobs: `BV_BENCH_FRAMES/WARMUP/DRAWS/INSTANCES`.

## Baseline (2026-07-14, M5 Pro host, fixed renderer)

300 frames, 100 draws/frame, 4 instances/draw, 8-tap sin/cos PS:

- first process of a fresh boot: **155.18 fps** (elapsed 1933 ms,
  15,518 draws/s), center pixel verified `3ef8f8ff`;
- second process: **156.63 fps** — within 1%, so no first-process penalty
  and no warm-up asymmetry remains after the first-draw fix;
- the draw smoke still passes as the third process of the same boot; the
  P3 trace gate passes with zero error responses.

Every draw carries a constant-buffer upload, so ~15.5k uploads/s including
the per-upload host flush are sustained inside that figure. Use this bench
(not WinSAT) to judge renderer-side optimizations, e.g. batching the
upload flush per submit instead of per upload.

## Stress ledger (run19, single continuous boot)

41 consecutive guest processes — one legacy-shader bench, twenty standard
benches, twenty draw smokes, alternating — all exit 0 with a clean agent
shutdown and NVMe writeback. Standard-bench distribution: median
154.9 fps, 18/20 runs in 127–159, plus two latency excursions (67.08 and
1.35 fps — a ~100x stall on one run). The excursions are a
perf-stability lead worth chasing with host-side timing once the renderer
is otherwise idle. The legacy bench ran at 161.56 fps with 16x the fill
(64 instances), so the workload is draw-call/transport-bound, not
fill-bound, at this scale.

## Found along the way (open leads)

- **Intermittent vrend shader-compile failure:** in run17 the bench's
  original shader pair (VS reading `SV_InstanceID` with `fmod`/`floor` and
  a ternary; PS reading `SV_POSITION`) failed host-side with
  `vrend_compile_shader: Illegal shader 0` on BOTH fresh contexts, and
  every draw then failed `DRAW_VBO: 104`. The SAME pair, preserved behind
  `BV_BENCH_SHADER=legacy`, compiled and ran at full speed in run19 — so
  the failure is state-dependent, not construct-deterministic, which makes
  it a nastier game-compatibility lead. When it recurs, capture with
  `VIRGL_LOG_LEVEL=debug VREND_DEBUG=shader` (the dumps are emitted below
  the release build's default warning threshold — plain `VREND_DEBUG=shader`
  logs nothing).
- A large (4 MB) staging readback works: the earlier all-zero reads came
  from the shader failure plus an unfenced `Map`, not from the
  bounce-buffer path.
- `Map` on staging resources does not block on GPU completion on this
  stack; guest code must fence with an EVENT query. Worth checking against
  WDDM contract expectations eventually (apps assume Map blocks).

## Evidence

`~/BridgeVM/viogpu3d-firstdraw-fix-20260714-v1/run15-perf-stress` (winsat
exit-0 + flat 42 record, 10/10 consecutive draw-smoke passes),
`run17-bench-fenced` (shader-failure forensics: 334 nonempty submits per
bench context, 5.09 MB payload each, `Illegal shader 0`),
`run18-bench-v2` (the baseline above).
