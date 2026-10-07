# Native VMM virtio-gpu 3D blob bounds — 2026-10-07

Integration starts from main `3609eb32`.

## Defects

The virtio-gpu 3D device keeps host-side bookkeeping beside virglrenderer.
Three guest-reachable defects were found in it, two by review and one by a
fuzz harness over the 3D command path.

- **Stale context attachments.** `ctx_resources` records which resources each
  live context has attached. `RESOURCE_UNREF` destroyed the resource but left
  its id in every context set. virglrenderer's
  `virgl_renderer_resource_unref` detaches the resource from every context
  before removing it, so the bookkeeping disagreed with the renderer. A reused
  id also looked attached to a context that never attached it, and
  `CTX_DESTROY` uses that answer to decide whether to unbind the blob scanout.
- **Unbounded destroyed-blob ids.** `destroyed_blob_mapped_ids` and
  `destroyed_blob_unmapped_ids` remember destroyed blob ids. They only choose
  the reason in a trace line and reject counter for a late
  `RESOURCE_UNMAP_BLOB`, and were cleared only on device reset.
- **Map-size overflow abort.** `RESOURCE_MAP_BLOB` rounded the blob's
  guest-declared `size` up to the 16 KiB HVF page with an unchecked multiply.
  Release builds enable overflow checks, so a size within a page of
  `u64::MAX` aborted the VMM process. The size is accepted at
  `RESOURCE_CREATE_BLOB`: the create path rejects only zero, and
  virglrenderer stores `map_size = args->size`. Its Venus Metal shared-memory
  export path (`vkr_device_memory_export_blob`) does not compare the size
  with the allocation.

A guest repeating create, attach and unref with fresh ids grew the first two
kinds of state without limit while one context stayed alive. A probe reached
50,000 stale context entries and 50,000 destroyed ids with no live blob.

## Repair

- `unref_resource` moved from `virtio_gpu_3d/resource_3d.rs` to
  `virtio_gpu_3d/resource_unref.rs`. It now removes the id from every context
  set, as virglrenderer does.
- Each destroyed-blob set keeps at most `MAX_DESTROYED_BLOB_IDS` (4096) ids
  and evicts its lowest id first. Command responses do not depend on these
  sets.
- `RESOURCE_MAP_BLOB` computes the footprint with `checked_next_multiple_of`
  and rejects a size without one as an invalid parameter, which is what an
  oversized blob already received from the shm-window check.
- The unmap request handler and its reject counters moved from
  `blob_host_mapping.rs` to `blob_unmap.rs`. `resource_3d.rs` drops from 180
  to 152 lines and `blob_host_mapping.rs` from 278 to 228 lines.

## Evidence

- `unref_detaches_the_resource_from_every_context` checks that an unref'd
  resource is no longer attached, and that a reused id is attached only after
  a new `CTX_ATTACH_RESOURCE`. It fails without the detach.
- `resource_churn_keeps_context_and_destroyed_id_bookkeeping_bounded` runs
  4608 create, attach and unref rounds. It checks that the context set is
  empty, that 4096 destroyed ids remain, and that the latest one is still
  recorded. It fails without the detach, and fails with 4608 ids without the
  cap.
- `map_blob_rejects_a_size_without_a_page_rounded_footprint` maps blobs of
  size `u64::MAX` and `u64::MAX - 0x3ffe`. With the unchecked rounding it
  panics with `attempt to multiply with overflow`.
- A throwaway fuzz harness (not committed) drove the 3D commands, backing
  attach and detach, unref, reset and scanout present against the mock
  backend. With the map overflow repaired, 128 seeds of 40,000 operations
  raised no panic. Coverage included accepted context, blob, 3D resource,
  transfer, submit, map and unmap commands, and at least 579 local
  resource-copy submits.
- `bridgevm-hvf` lib tests pass 1350/0 with one existing ignore. The
  `virtio_gpu` tests pass 142/0 with `venus`. Clippy is clean with and without
  `venus`.

## Scope

Responses, renderer calls and scanout handling are unchanged for guests that
detach resources before unref and declare representable blob sizes. A11
remains OPEN. Capability states, thresholds, known defects and product
wording are unchanged, and there is no live Windows or release claim.
