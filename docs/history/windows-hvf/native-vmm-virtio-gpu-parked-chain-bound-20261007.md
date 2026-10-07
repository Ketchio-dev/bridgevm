# Native VMM virtio-gpu parked-chain bound — 2026-10-07

This checkpoint records one deterministic device-model repair. It does not
establish a Windows workload, a physical-HVF run or a release gate. Product
state and all criteria remain owned by `capabilities/windows-hvf.json`.
Integration starts from main `05935e2d`.

## Defect

With host vblank pacing enabled (the runtime default is 120 Hz), the
virtio-gpu control queue holds each empty context-0 `SUBMIT_3D` response
until the next vblank. It retires one response per interval. Fence-parked
responses are held in the same way until their backend fence retires.

Each parked response keeps its descriptor list and a response buffer on the
host. The device consumed every newly published avail entry whatever the
number already parked, so a driver that kept republishing heads without
waiting for their completions grew the parked list without limit. A probe
reusing one head parked 1,000,000 responses (210 MB resident) in 147 ms.

## Repair

A conforming driver owns at most `queue size` heads, so more parked chains
than that means heads that are still in flight were republished. QEMU
v11.0.0 `virtqueue_pop` pops nothing once `vq->inuse >= vq->vring.num`.

BridgeVM now stops consuming control-queue avail entries while the parked
vblank and fence responses together fill the queue size. The remaining
entries stay in the ring and are consumed after a parked response retires.
QEMU additionally marks the device broken at that point. BridgeVM only stops
popping, which a conforming driver never observes, so this adds no
guest-visible deviation.

The check lives in the new `parked_chains.rs`, together with the moved
`write_used` helper. This lowers the `virtqueue.rs` budget from 398 to 388
lines.

## Discovery and verification

The issue was found by static review of host-side queues fed by guest
activity, after the virtio-console and NAT backlog findings. It was
confirmed with an uncommitted probe, which held 16 parked responses at
8.1 MB resident after the repair.

The fixture `virtio_gpu/tests/parked_chains.rs` submits 20 paced
submissions on a 16-entry queue without retiring them. It expects 16 parked
responses and 16 consumed entries, and checks that retiring one response
frees one slot. The original implementation parks 20.

All 139 `virtio_gpu` tests pass in Release, and the `bridgevm-hvf` library
passes 1344/0 with one existing ignore.

## Boundary

Only control-queue consumption while the parked chains fill the queue
changed. Vblank pacing cadence, fence retirement, cursor-queue processing
and snapshot handling are unchanged. No firmware boot or Windows run was
performed for this repair.
