# Native VMM NAT reply and write-backlog bound — 2026-10-07

This checkpoint records one deterministic device-model repair. It does not
establish a Windows workload, a physical-HVF run or a release gate. Product
state and all criteria remain owned by `capabilities/windows-hvf.json`.
Integration starts from main `92006c65`.

## Defect

The userspace NAT behind virtio-net had two host-side queues that grew with
guest activity and had no limit.

- **Reply queue.** ARP, DHCP and gateway ICMP replies were appended to the
  NAT reply queue for every guest request. That queue drains only into
  guest RX buffers, so a guest that kept sending requests without posting RX
  buffers grew host memory with every request. A probe sending ARP and ICMP
  echo requests to the gateway queued 4 million replies, reaching 298 MB
  resident in 77 ms. Host socket polling also appended UDP, TCP and ICMP
  traffic to this queue with no check on its length.
- **TCP write backlog.** Each in-order guest TCP segment was appended to the
  flow's host write buffer and acknowledged, even when the host peer had
  stopped reading. A guest could keep sending to a host service that did not
  read, and the buffer grew without limit.

## Repair

QEMU v11.0.0 `net/queue.c` `qemu_net_queue_append` drops a packet when the
queue already holds `nq_maxlen` (10000) packets and no completion callback is
attached.

- **Reply queue.** BridgeVM now caps the reply queue at the same 10000 frames.
  Replies synthesized from guest requests are dropped once it is full. While
  it is full, the NAT does not poll host sockets, and TCP host reads stop.
  Host data stays in the kernel socket buffers rather than being read and then
  dropped, because this NAT never retransmits a segment it has dropped.
  Pending connection-refused resets are capped at the same count.
- **TCP write backlog.** An in-order segment that arrives while 4 MiB is
  already buffered for the host is dropped without acknowledgement, FIN
  included. The guest TCP stack retransmits it from the last acknowledged
  byte once the host peer reads again.

`QueuedOutboundIpv4Handler` moved to `queued_outbound.rs`, and the bounds
live in `reply_queue.rs`. This lowers the `macaddr.rs` budget from 902 to 881
lines.

## Discovery and verification

The issue was found by a static review of guest-fed host queues, after the
virtio-console backlog finding. It was confirmed with an uncommitted probe.

Two new fixtures cover the bounds:

- 10000 ARP and 10000 gateway ICMP requests without draining leave exactly
  10000 queued replies. Every request is still counted, and draining one frame
  makes room for the next reply.
- An established loopback flow already holding 4 MiB leaves a further
  in-order segment unacknowledged and unbuffered.

The original implementation fails both: it queues 20000 replies, and buffers
the extra segment while advancing the acknowledged sequence. With the
repair, the probe holds 10000 replies at 8.4 MB resident. All 44 `net_nat`
tests pass in Debug and Release, and the `bridgevm-hvf` library passes 1343/0
with one existing ignore. An uncommitted 2000-seed, 5000-step NAT frame fuzz
run of the repaired code found no panic.

## Boundary

Only the two host-side backlogs changed. Frame parsing, flow tables, idle
eviction and virtio-net RX delivery are unchanged. No live network transfer,
firmware boot or Windows run was performed for this repair.
