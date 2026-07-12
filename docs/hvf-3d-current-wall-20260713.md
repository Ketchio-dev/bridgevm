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

The remaining Windows work is:

1. add a repeatable guest D3D10/OpenGL workload and result gate rather than
   relying only on desktop composition and `dxdiag` capability reporting;
2. run long-duration graphics stress and recover cleanly from renderer/device
   failures;
3. replace the disposable test-signed package flow with reproducible,
   distributable driver provenance/signing and normal product update UX;
4. move beyond the current feature-level-10_0 ceiling before claiming modern
   DX11/DX12 game compatibility.

Live pacing evidence is preserved at
`~/BridgeVM/viogpu3d-scanout-pacing-proof-20260713-v1`. Its regenerated trace
report survives device reset and passes the VirGL P3 gate with zero errors.

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
and records the setting in every Linux Venus preflight. The next useful work is
GPU-timestamp-backed profiling and renderer/Metal transfer-path experiments;
inflating the old narrow microbenchmark would not be a real improvement.

Live default evidence is preserved at
`~/BridgeVM/venus-p2/live-copy-bench-v1/evidence`; the heap-off comparison is at
`~/BridgeVM/venus-p2/mtlheap-ab/off/evidence-full`.
