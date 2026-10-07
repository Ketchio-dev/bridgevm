# Native VMM virtio-gpu 2D host-memory budget — 2026-10-07

This checkpoint records one deterministic device-model repair. It does not
establish a Windows workload, a physical-HVF run or a release gate. Product
state and all criteria remain owned by `capabilities/windows-hvf.json`.
Integration starts from main `d668f64b`.

## Defect

`RESOURCE_CREATE_2D` checked `width * height * 4` only for `u64` and `usize`
overflow. It then allocated a zero-filled host pixel buffer of that size. A
guest could request, for example, 0x403c1a21 × 0x77943681 pixels, about
8.6 × 10¹⁸ bytes. The allocation failed and Rust aborted the whole VMM process
with SIGABRT. Smaller but still very large requests, repeated across resource
ids, had no total bound either.

## Repair

QEMU v11.0.0 (`hw/display/virtio-gpu.c`) admits a 2D resource only while
`res->hostmem + g->hostmem < max_hostmem`, which defaults to 256 MiB. It
answers both the size overflow and the budget refusal with
`VIRTIO_GPU_RESP_ERR_OUT_OF_MEMORY`.

BridgeVM now applies the same strict 256 MiB bound to the sum of 2D resource
pixel buffers, and returns the same response for an over-budget or
overflowing create. Re-creating an existing resource id replaces that
resource's pixels, so the old pixels are not counted twice. `RESOURCE_UNREF`
and device reset release the memory. The largest resize scanout
(7680 × 7680) is about 225 MiB, so it still fits as one resource. Matching
QEMU adds no guest-visible deviation.

## Discovery and verification

An uncommitted structured virtio-gpu control-queue fuzz harness aborted on its
seventh seed. The cause was this allocation, which was attributed by its
backtrace through `resource_create_2d_into`.

Two new queue-level fixtures:

- An oversized create, a `u32::MAX` create, and an exactly-256-MiB create are
  all refused, and no resource is created.
- Two ~127 MiB resources fit and a third is refused. Replacing an id does not
  add to the total, and `RESOURCE_UNREF` frees room.

The original implementation aborted the test process with
`memory allocation of 8648180421474943620 bytes failed`, SIGABRT, in Debug and
Release. The repair passes 2/0 in both. The `bridgevm-hvf` library passes
1334/0 with one existing ignore in Debug and Release. After the repair, the
harness ran 3,000 seeds of 5,000 commands on 2D and mock-3D devices with no
panic or abort.

## Boundary

This bounds only 2D resource pixel storage. Blob and 3D resource memory are
owned by the Venus backend and its own checks. No live viogpu3d or viogpudo
run was performed. A guest that creates more than 256 MiB of 2D resources at
once now receives `ERR_OUT_OF_MEMORY`, as it does on QEMU.
