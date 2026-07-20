# Windows Venus host-visible memory map wall (2026-07-20)

## Status

Open. This is the next focused Venus wall after the 2026-07-19 display-start
resolution. It was exposed by the new in-guest Vulkan draw smoke; the desktop
D3D path is unaffected.

## What the new draw smoke proved first

`scripts/win-tests/bridgevm-vulkan-draw-smoke.c` is a dual-target C program:
the same source runs on the macOS host against MoltenVK and cross-compiles with
`zig cc` for the Windows ARM64 guest. It performs two gated offscreen stages
with a pixel-level readback assertion (gate C: `vkCmdClearColorImage` to red;
gate D: render-pass clear to blue plus an embedded-SPIR-V half-viewport green
triangle), with per-gate exit codes 30-37.

- Host validation passes completely: clear 4096/4096 red, draw green=2016 /
  blue=2080 / other=0, exit 0. The Vulkan logic and shaders are correct against
  the same MoltenVK the Venus backend uses.
- In the guest, stage 3 of the firstboot flow now runs the smoke after the
  enumerate-only probe. The 2026-07-20 bounded activation run reached:
  instance OK, `device_name=Virtio-GPU Venus (Apple M4 Max)`, graphics queue
  found, `vkCreateDevice` OK — then `vkMapMemory` on the 16 KiB host-visible
  readback buffer failed with `VK_ERROR_MEMORY_MAP_FAILED` (gate 34).

## Root cause chain

An earlier draft of this document hypothesized a virglrenderer export gap
(missing dma-buf/opaque-fd handles on macOS). That hypothesis is wrong and is
replaced by the diagnosis below; no host-side change is required.

Guest-side beacons show the Venus ring and reply shmem blobs map fine
("blob map escape ok", host trace: 124 `RESOURCE_MAP_BLOB` all `OK_MAP_INFO`).
The decisive evidence is the render-server log paired with the host command
order:

```text
virgl_render_server:
vkr: failed to import resource: invalid res_id 240
vkr: vkAllocateMemory resulted in CS error
vkr: ring_submit_cmd: vn_dispatch_command failed

virtio-gpu.jsonl (smoke process, ctx 30):
seq 2345..2367  SUBMIT_3D            (one of these carries the vkAllocateMemory
                                      CS that imports res_id 240)
seq 2368        RESOURCE_CREATE_BLOB res=240 blob_mem=HOST3D blob_id=9
                -> ERR_UNSPEC        (arrives only AFTER the submits)

run.log:
venus: resource_create_blob ctx=30 res=240 blob_mem=2 blob_id=9 size=16384 ret=-1
```

This is a **guest driver ordering race**, not a host export gap. The Mesa vn
WDDM winsys (`arehnman/virtio-win-mesa`,
`src/virtio/vulkan/vn_renderer_d3dkmt.c`,
`virtgpu_d3dkmt_resource_create_blob`) creates the blob resource with
`D3DKMTCreateAllocation` and immediately submits the `vkAllocateMemory` CS
that imports that res_id. In WDDM, `D3DKMTCreateAllocation` returns as soon as
the KMD-side allocation object exists; the host-bound `RESOURCE_CREATE_BLOB`
control command is emitted later on an asynchronous (paging/deferred) path. So
the CS import reaches the render server before the resource exists, the
allocation fails, and the subsequent `RESOURCE_CREATE_BLOB` export of
`blob_id=9` (the memory object that was never created) fails with
`ERR_UNSPEC`; the guest then sees `VK_ERROR_MEMORY_MAP_FAILED`.

Ring/reply blobs (`blob_id=0`) are ordered correctly (seq 2342 create → 2343
attach → submits) because their creation is immediately followed by the
`VIOGPU_RES_MAP_BLOB` escape, which forces synchronization with the host.

The vendored virglrenderer patch
(`scripts/patches/virglrenderer-macos-venus.patch`) already implements the
macOS host-visible allocation path — shm-backed allocation imported via
`VK_EXT_external_memory_host` host pointer, exported as
`VIRGL_RESOURCE_FD_SHM` — and MoltenVK advertises both
`VK_EXT_external_memory_host` and `VK_EXT_external_memory_metal`, so the host
side is ready once the guest ordering is fixed.

The follow-on attach/unmap/detach failures (seq 2369-2372) are cleanup noise
from the same failed create; the new `unmap_blob_reject_counts()` labels them
`never_created`.

Until now nothing exercised this: the working desktop D3D/WDDM path uses only
guest-memory-backed blobs. Every guest Vulkan workload that needs staging or
readback memory hits this wall, so it gates all real Vulkan content.

## Next work

Fix the guest driver ordering so the host observes `RESOURCE_CREATE_BLOB`
before any CS that imports the resource:

1. Preferred (KMD, `anonymix007/kvm-guest-drivers-windows-venus`
   `viogpu3d-venus-wip`): make `DxgkDdiCreateAllocation` (or the
   `VIOGPU_RES_INFO` escape that returns the res id) submit the
   `RESOURCE_CREATE_BLOB` synchronously and wait for host completion, reusing
   the same synchronization the `VIOGPU_RES_MAP_BLOB` escape already performs.
2. Fallback (UMD): add a create-flush escape at the end of
   `virtgpu_d3dkmt_resource_create_blob` so the winsys guarantees the create
   landed before returning.

Build the fix as driver 120.36 through the established
`Ketchio-dev/viogpu3d-arm64-builder` CI loop, then rerun the bounded
activation; the draw smoke's gate C/D assertions define done.

## Guest-run pipeline fixes landed alongside (2026-07-20)

Three firstboot-flow defects surfaced while driving the runs; all are fixed in
`scripts/win-assets/`:

- `bvgpu-firstboot.cmd` is now CRLF. The repository's LF-only copy tripped
  cmd's label scanner (offset-dependent): `call :require_new_boot` failed
  instantly with no log line once unrelated edits shifted byte offsets. CRLF
  removes the entire failure class; keep this file CRLF.
- Stage boot-identity files are written with PowerShell
  `FileStream(WriteThrough)+Flush(true)`. A plain cmd redirect left the tiny
  NTFS-resident file unflushed, and the stage reboot persisted a
  correct-length, all-NUL file, wedging every later boot at the stage2 gate.
  The gate now also accepts an unreadable-but-present boot file (existence
  alone proves the stage handoff) instead of wedging.
- The vulkan probe log has a single writer again: the diagnostics runner's
  redirect now goes to `bvgpu-vulkan-probe.log.console` instead of racing the
  probe's own appends (closes the 2026-07-19 dual-writer note).

The evidence chain for the 2026-07-20 wall is under
`/Users/insighton/BridgeVM/runs/venus-activate-120.35-draw-smoke5-*` (final
run: stage3 probe PASS, draw smoke gate 34, host trace with the ERR_UNSPEC
create-blob), with the WinPE asset-refresh injects in the sibling
`venus-inject-120.35-draw-smoke*` directories.

## Host device-model instrumentation added

`crates/bridgevm-hvf/src/virtio_gpu_3d.rs` now classifies
`RESOURCE_UNMAP_BLOB` invalid-parameter responses
(`already_destroyed_was_mapped` / `already_destroyed_was_unmapped` /
`never_created`, plus `short_request`) with venus-start trace lines, id-reuse
lifecycle handling, and `unmap_blob_reject_counts()` for reports; unit-tested.
This separates the guest driver's late-unmap cleanup noise from real
mapping-lifecycle bugs when reading future traces.
