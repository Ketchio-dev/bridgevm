# Native VMM virtio-console backlog bound — 2026-10-07

This checkpoint records one deterministic device-model repair. It does not
establish a Windows workload, a physical-HVF run or a release gate. Product
state and all criteria remain owned by `capabilities/windows-hvf.json`.
Integration starts from main `20769f6e`.

## Defect

Two host-side virtio-console queues grew with guest activity and had no limit.

- **Control backlog.** Each `DEVICE_READY(1)` the guest sent on control TX
  queued two `DEVICE_ADD` replies. While the guest posted no control-RX
  buffers, the replies stayed in `pending_control`. A probe that kept
  re-submitting the same 256-entry control-TX ring queued 25.5 million
  messages, about 1 GB, in 0.36 s.
- **Agent inbound bytes.** Agent TX appended every guest chain to
  `host_inbound`, up to 64 KiB per chain. The product drains it only from the
  agent harness tick, and that tick stops after the harness reaches `Done` or
  `TimedOut`. After that, a guest writing to the agent port grew host memory
  without limit. A fuzz harness that drained only occasionally received
  235 MB in total.

## Repair

QEMU v11.0.0 (`hw/char/virtio-serial-bus.c`) does not queue control messages.
`send_control_msg` pops a control-RX element and returns without sending when
none is available. Port data uses throttling: `do_flush_queued_data` stops
popping guest TX elements while the port is throttled, and resumes after the
backend unthrottles.

- **Control backlog.** BridgeVM keeps a backlog of at most 64 control messages
  and drops new ones beyond it. Ordinary bring-up queues at most six (two
  `DEVICE_ADD`, `PORT_NAME`, `PORT_OPEN` and one re-assert pair), so the
  existing bring-up race handling is unchanged. A guest that needs a dropped
  message is in the same position as under QEMU, which never queued it.
- **Agent inbound bytes.** Agent TX stops consuming descriptors while 4 MiB of
  bytes remain undrained. The chains stay in the ring without a used entry,
  and a later poll consumes them once the host drains. This is the same
  backpressure a throttled QEMU port applies. The largest agent reply is one
  16 MiB command output, delivered in base64 lines; the harness drains on
  every service tick, so the bound throttles only an undrained buffer.

`PendingControlMessage` and the enqueue path moved to `control_message.rs`,
and the inbound accessors moved to `agent_inbound.rs`. This lowers the
`control_plane.rs` budget from 242 to 184 lines and the `agent_data_path.rs`
budget from 99 to 94.

## Discovery and verification

The issue was found with a throwaway virtio-console fuzz harness that
instrumented total inbound bytes. A follow-up probe measured the control
backlog. Neither harness is committed.

Two new fixtures cover the bounds:

- 8000 `DEVICE_READY` chains with no control-RX buffer leave exactly 64 queued
  messages, and every chain still completes.
- Undrained 64 KiB agent chains stop at 4 MiB of inbound bytes. After a drain,
  the next poll consumes the waiting chains.

The original implementation fails both, queueing 16000 messages and
524288000 inbound bytes. With the repair, the probe peaks at 7.8 MB resident
instead of 1.03 GB. All 24 virtio-console tests pass in Debug and Release,
and the `bridgevm-hvf` library passes 1340/0 with one existing ignore. A
3000-seed, 20000-step fuzz run of the repaired device found no panic.

## Boundary

Only the two host-side backlogs changed. Port-0 console traffic,
host-to-guest agent sends, control-TX parsing and snapshot encoding are
unchanged. No firmware boot or Windows run was performed for this repair.
