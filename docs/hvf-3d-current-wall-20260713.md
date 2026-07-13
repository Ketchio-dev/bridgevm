# HVF 3D current wall — 2026-07-13

BridgeVM now has live, no-QEMU GPU execution on both target guests:

- Windows 11 ARM64 binds the test-signed `viogpu3d` VirGL package, reports
  WDDM 1.3 / D3D feature level 10_0, renders the desktop, accepts live input,
  and passes the protocol-specific command/fence trace gate.
- Fedora ARM64 executes Vulkan through Venus → virglrenderer → MoltenVK and
  verifies device-local buffer and optimal-tiled image results by readback.

## What is no longer the wall

- Windows PCI binding, VirGL capsets, 3D resources, contexts, non-empty submit,
  renderer fences, scanout, app display export, input, and bundled runtime have
  all passed live.
- Linux's zero-low 64-bit BAR mapping bug is fixed. The guest can map the
  1 GiB host-visible BAR above 4 GiB and execute the corrected benchmark.
- The old 136.14 number was a 16 MiB overwrite microbenchmark and was labelled
  with the wrong binary/decimal unit. It is not a comparable 128 MiB baseline.

## Windows: the present wall

The 2026-07-13 paced run rendered a full 1280x800 3D scanout by 14.419 seconds.
The resident agent declared the desktop milestone at 40.731 seconds, so that
26-second gap is not renderer latency.

The same run observed 692 full scanout readbacks and coalesced 196 additional
flushes (22.07%) with the new 16 ms default. Readback work totalled 0.505 seconds
at 5.616 GB/s, only about 1.24% of the 40.731-second agent-ready interval.
Scanout readback was worth bounding, but it is not the dominant Windows wall.

The resident service has now also completed the built-in `winsat d3d` suite
with exit code zero. WinSAT invoked DX9- and DX10-class batch, alpha, texture,
ALU, geometry, and constant-buffer assessments. The same boot produced 4,895
valid VirGL trace events, non-empty `SUBMIT_3D`, completed fences, and no GPU
error responses, so this is the first repeatable real D3D workload gate rather
than capability-reporting evidence alone. Every reported subtest result was the
same 42 F/s, however, so those scores are not accepted as performance numbers.

The BridgeVM-owned D3D10 clear probe now separates two behaviors. A
64x64 default-texture initialization/copy/readback returns the expected
`112233ff` pixels. With the RTV explicitly bound, clear/copy/readback returns
the expected `4080bfff` pixels with zero bad pixels and exits zero. This closes
the owned API-result gate, but the owned context still produces no non-empty
`SUBMIT_3D` in the host trace, so it is not proof that the host renderer
executed that clear. A stronger owned draw probe compiles VS/PS 4.0 HLSL in the
guest, binds a vertex buffer, and draws a fullscreen magenta triangle. Its
readback remains black (`center=000000ff`, zero magenta pixels), while its
context creates and attaches the expected target, staging, vertex, and command
resources but emits no non-empty submit. This makes the active wall the
Mesa/WDDM command-submission boundary rather than shader compilation, resource
creation, or host renderer correctness.

Source inspection identified a separate Mesa VirGL correctness defect: its
`clear_render_target` encoder emits nothing when the target surface is not in
the current framebuffer. BridgeVM now carries a pinned patch that creates a
temporary VirGL surface for that legal D3D10 case, and the Windows ARM64 build
kit applies it with `git apply --check` before compiling Mesa. The probe's
`--unbound` mode is the regression gate for the rebuilt UMD.

The remaining Windows work is:

1. rebuild/install the patched UMD and make the owned `--unbound` readback pass;
2. instrument and repair the UMD/KMD render call so the existing owned
   draw/shader workload emits a non-empty `SUBMIT_3D` and returns magenta;
3. obtain meaningful performance timing rather than WinSAT's flat 42 F/s
   compatibility result;
4. run long-duration graphics stress and recover cleanly from renderer/device
   failures;
5. replace the disposable test-signed package flow with reproducible,
   distributable driver provenance/signing and normal product update UX;
6. move beyond the current feature-level-10_0 ceiling before claiming modern
   DX11/DX12 game compatibility.

Live pacing evidence is preserved at
`~/BridgeVM/viogpu3d-scanout-pacing-proof-20260713-v1`. Its regenerated trace
report survives device reset and passes the VirGL P3 gate with zero errors.
The WinSAT D3D workload proof is preserved at
`~/BridgeVM/viogpu3d-winsat-d3d-proof-20260713-v1`.
The owned bound-clear proof is preserved at
`~/BridgeVM/viogpu3d-owned-d3d10-smoke-20260713-v8-bound-live`.
The owned draw failure and exact no-submit trace are preserved at
`~/BridgeVM/viogpu3d-owned-d3d10-draw-20260713-v1`.

## Linux: the present wall

The corrected live benchmark uses a 128 MiB device-local working set and 1 GiB
of verified work. On the Apple M5 Pro host it measured:

- `vkCmdFillBuffer`: 105.91 GB/s (98.64 GiB/s);
- dependent `vkCmdCopyBuffer`: 117.14 GB/s (109.09 GiB/s).

A device-local copy reads and writes every logical byte, so 117.14 GB/s of
logical copies represents roughly 234.28 GB/s of memory traffic. This points
at the MoltenVK/Metal memory path, not Venus command transport, as the current
ceiling.

Disabling `MTLHeap` reduced copy throughput to 98.06 GB/s while fill remained
107.90 GB/s. BridgeVM therefore keeps MoltenVK's heap-backed allocation enabled
and records the setting in every Linux Venus preflight.

The benchmark now also requests Vulkan timestamp queries and preserves the raw
counter values. The live path reports timestamp support, 64 valid bits, and a
1 ns period, but only the first query receives a counter; the later query in the
same submission returns zero for both `BOTTOM_OF_PIPE` and `TRANSFER`. BridgeVM
therefore reports `gpu_valid=0` instead of converting the underflow into a fake
GPU bandwidth. Until that Venus/virglrenderer/MoltenVK query-path defect is
fixed, wall-clock fence timing remains the trustworthy measure. Repeated live
wall-clock copies reached 120-124 GB/s, while fills varied more widely and are
not yet a stable optimization target.

The next useful work is to repair or bypass the timestamp-query path, then use
the resulting GPU/submission-time split for renderer and Metal transfer-path
experiments; inflating the old narrow microbenchmark would not be a real
improvement.

Live default evidence is preserved at
`~/BridgeVM/venus-p2/live-copy-bench-v1/evidence`; the heap-off comparison is at
`~/BridgeVM/venus-p2/mtlheap-ab/off/evidence-full`; raw timestamp evidence is at
`~/BridgeVM/venus-p2/live-gpu-timestamp-v1/evidence-transfer-timestamps`.
