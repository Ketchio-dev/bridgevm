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

## Found along the way (open leads)

- **vrend shader-translation hole:** the bench's first shader pair failed
  host-side with `vrend_compile_shader: Illegal shader 0` on both fresh
  contexts, and every draw then failed `DRAW_VBO: 104`. Constructs in the
  failing pair (any could be the trigger): VS reading `SV_InstanceID` with
  `fmod`/`floor` grid math and a ternary; PS reading `SV_POSITION`.
  Replacing them with a CB-driven VS (no instance id, no fmod/floor) and a
  TEXCOORD-only PS compiles and runs. Real D3D10 titles will hit such
  constructs — this is a game-compatibility wall to chase with
  `VREND_DEBUG=shader` (prints the failing TGSI/GLSL) before any DX11
  ambitions.
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
