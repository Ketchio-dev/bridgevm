# GPU device thread — design and deferral rationale (2026-07-21)

## Current threading model (verified against source)

- All virglrenderer FFI runs on whichever vCPU thread services the
  serialized virtio-gpu queue notification. vrend's CGL context migrates
  between vCPU threads via `CGLLockContext` + rebind
  (virglrenderer-macos-venus.patch, vrend_decode.c) — Apple permits a
  context to migrate as long as its use is bracketed.
- Venus/vkr (MoltenVK) runs in the forked `virgl_render_server` child
  process; only vrend GL and the proxy socket live in the probe.
- Fence retirement: `virgl_renderer_context_poll` sweeps every context on
  the three vCPU-thread drain sites (notify tail, post-access, per-exit).
  `VIRGL_RENDERER_THREAD_SYNC`/`ASYNC_FENCE_CB` are accepted but no sync
  thread exists on macOS — the flags are inert hints.

## What this rung was for, and what already landed instead

The audit's target was recovering vCPU-thread time burned on GPU work.
The dominant item — the 3 ms/frame synchronous scanout readback — is now
addressed twice over: deferred servicing moved it off the guest present
path, and the IOSurface GPU blit (92 us avg) replaces it on the display
path entirely, with the CPU readback demoted to a pace-able evidence feed.

Remaining vCPU-thread GPU costs:
1. `SUBMIT_3D` handling — real GL decode work for virgl contexts (DWM's
   D3D10 composition) and proxy forwarding for venus contexts. Now
   measured: the `command` trace event carries `duration_ns` as of this
   change, so the next run quantifies exactly how much vCPU time submits
   burn. This is the number that justifies (or kills) the refactor.
2. `poll_fences` all-context sweeps — cheap FFI per drain, previously
   audited as second-order.
3. The IOSurface blit — ~92 us per displayed frame.

## Design (when the submit numbers justify it)

A dedicated GPU thread owns every virglrenderer call:

- One SPSC ring from the device (vCPU threads) to the GPU thread carrying
  decoded commands: submits, flush-blits, fence creates, resource ops.
- Fire-and-forget ops (SUBMIT_3D, blits, fence create) return to the guest
  immediately — the virtqueue response no longer waits for GL execution.
  This is the same decoupling the deferred scanout proved safe, extended
  to submits.
- Result-bearing ops (map_blob, capset queries, resource create) block the
  calling vCPU thread on a reply slot; they are rare and init-heavy.
- The CGL context binds once on the GPU thread and stops migrating; the
  CGLLock bracket collapses to plain use. Fence retirement moves to the
  same thread's idle loop, which also unblocks the audit's P1b host
  fence-poll item (181→138 us headroom) for free.
- Ordering: the ring preserves per-queue order, which is the virtio
  contract; fences complete in submit order per context as today.

## Why deferred now

- The measured pain this rung addressed is gone (readback), and the
  remaining big-ticket item (submit decode) is unquantified until a run
  with `duration_ns` lands. Refactoring the device threading model on an
  unmeasured cost inverts the project's measure-first rule.
- The refactor moves the virtqueue response semantics for submits; DXVK
  and the KMD vsync NOP path (empty context-0 submits driving CRTC_VSYNC)
  are sensitive to completion timing, so this needs its own validation
  ladder, not a tail-end change.

Gate to reopen: a real-title run showing SUBMIT_3D `duration_ns` totals
that are a material share of wall time (>5%/s of a core) on the vCPU
thread.
