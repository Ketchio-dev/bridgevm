# Windows Venus host-visible memory map resolution (2026-07-20)

## Status

Resolved. The Windows ARM64 guest now allocates, maps, writes, and reads back
host-visible Vulkan memory through the Venus path, and the in-guest draw smoke
passes both image-level gates with **byte-identical checksums to the same
smoke executed natively on the host against MoltenVK**:

```text
gate_clear=PASS  red_pixels=4096/4096  checksum=0x79d66447807a4325
gate_draw=PASS   green=2016 blue=2080 other=0
                 inside=0,255,0,255 outside=0,0,255,255
                 checksum=0x0d6d2a0be685f2e5
success (exit 0); firstboot stage 3 completed and deleted its ONSTART task
```

This closes the wall documented in
`hvf-venus-hostvisible-map-wall-20260720.md` and completes the first
end-to-end proof that guest Windows Vulkan rendering is pixel-exact against
host Metal via Venus → virtio-gpu → MoltenVK.

## The two root causes and their fixes

The wall had two stacked causes, one per side of the device boundary.

### 1. Guest KMD create/submit ordering race — driver 120.36

Mesa's D3DKMT winsys mirrors Linux DRM semantics: it assumes the blob
resource exists host-side once `virtgpu_d3dkmt_resource_create_blob` returns,
then may ring-submit a `vkAllocateMemory` whose
`VkImportMemoryResourceInfoMESA` references that res id.  The KMD deferred
`RESOURCE_CREATE_BLOB` + `CTX_ATTACH_RESOURCE` to `VioGpuDeviceAllocation`
open (first device use), which runs after those submits, so the render server
saw the import first: `vkr: failed to import resource: invalid res_id`.

Fix (builder branch `fix/venus-create-blob-sync`, package `120.36.0.0`, CI run
`29764429482`, patch `viogpu3d-venus-create-blob-sync.patch`): the Mesa-form
`VIOGPU_RES_INFO` escape now creates and context-attaches the blob before it
returns — `CtrlQueue::CreateResourceBlob` is already synchronous, and
control-queue FIFO order then guarantees the host observes the resource before
any importing submit.  The device-open path consumes the recorded early attach
so attach/detach stays balanced.

Verified by trace order: `RESOURCE_CREATE_BLOB` (seq 10765) now precedes the
allocating `SUBMIT_3D` (seq 10768), where under 120.35 the create trailed the
submits.

### 2. Host renderer lacked the guest-vram-style create/import path

With ordering fixed, `vkr_context_create_resource` still failed the create:
for `blob_id != 0` it only implemented upstream export semantics (look up an
existing `VkDeviceMemory`), but the memory object is allocated *after* the
create in this flow.  The capset advertises `use_guest_vram`, so the guest
legitimately creates the mappable blob first and lets `vkAllocateMemory`
import it.

Fix (vendored virglrenderer, carried in
`scripts/patches/virglrenderer-macos-venus.patch`, rebuilt via
`scripts/build-venus-host-deps.sh`):

- `vkr_context_create_resource`: a `USE_MAPPABLE` create whose `blob_id`
  object does not exist yet is serviced as a shm allocation (same storage
  class as `blob_id == 0` ring blobs).
- `vkr_dispatch_vkAllocateMemory`: the resource-import translator gains a
  `VIRGL_RESOURCE_FD_SHM` case that imports the resource's pages via
  `VkImportMemoryHostPointerInfoEXT` (`VK_EXT_external_memory_host`, already
  enabled at device creation by the existing macOS patch), rounding
  `allocationSize` up to the shm allocation size.  MoltenVK wraps the pages
  NoCopy, so GPU writes land in the same shm the guest maps through the
  shared-memory BAR.

No BridgeVM device-model change was needed: the existing
`map_blob` → `HvGpuShmMapPort` (hv_vm_map) path served the new blobs as-is.

## Verification (evidence)

Final run: `/Users/insighton/BridgeVM/runs/venus-activate-120.36-shm-import-*`
(240 s watchdog; stage 3 rerun via the persistent ONSTART task, then the task
deleted itself after success).

- Guest: `C:\BridgeVM\bvgpu-vulkan-draw.log` as quoted above; installed
  `oem36.inf` reports `120.36.0.0`.
- Host: zero `resource_create_blob ... ret=-1`, zero render-server
  `invalid res_id`; `blob_id != 0` `RESOURCE_CREATE_BLOB` responses OK;
  P3 Windows 3D trace gate PASS with no blockers.
- Desktop scanout unchanged (visible Windows 11 desktop in the 90 s
  virtio-gpu checkpoint frame) — the `viogpudo`/WDDM display path did not
  regress.
- Residual `RESOURCE_UNMAP_BLOB` invalid-parameter responses dropped to two
  early-boot occurrences, matching the `never_created` cleanup-noise class
  identified by the new `unmap_blob_reject_counts()` instrumentation.

## Remaining work

- Extend the draw smoke into a repeated draw/fence timing loop (poor-man's
  vkmark) for a transport/fence performance baseline, then advance the P3
  ladder rungs: DXVK d3d11 → a real DX11 title, then vkd3d-proton d3d12.
- Remove the temporary Mesa D3D beacon patches from the builder chain
  (deferred until now so they could assist this bring-up).
- The guest driver build branch advanced from `fix/venus-submit-escape` to
  `fix/venus-create-blob-sync`; future driver work should branch from the
  latter.
