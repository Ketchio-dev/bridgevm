//! virtio-console wire constants: device id, feature/queue ids, control events.

pub(crate) const DEVICE_ID_CONSOLE: u32 = 3;

pub(crate) const VIRTIO_CONSOLE_F_MULTIPORT: u32 = 1 << 1;

pub const QUEUE_PORT0_RX: usize = 0;

pub const QUEUE_PORT0_TX: usize = 1;

pub const QUEUE_CONTROL_RX: usize = 2;

pub const QUEUE_CONTROL_TX: usize = 3;

pub const QUEUE_AGENT_RX: usize = 4;

pub const QUEUE_AGENT_TX: usize = 5;

pub(crate) const QUEUE_COUNT: usize = 6;

pub(crate) const QUEUE_MAX: u16 = 64;

pub(crate) const PORT_COUNT: usize = 2;

pub(crate) const AGENT_PORT_ID: u32 = 1;

pub const AGENT_PORT_NAME: &[u8] = b"org.bridgevm.agent.0";

pub(crate) const VIRTIO_CONSOLE_DEVICE_READY: u16 = 0;

pub(crate) const VIRTIO_CONSOLE_DEVICE_ADD: u16 = 1;

pub(crate) const VIRTIO_CONSOLE_DEVICE_REMOVE: u16 = 2;

pub(crate) const VIRTIO_CONSOLE_PORT_READY: u16 = 3;

pub(crate) const VIRTIO_CONSOLE_CONSOLE_PORT: u16 = 4;

pub(crate) const VIRTIO_CONSOLE_RESIZE: u16 = 5;

pub(crate) const VIRTIO_CONSOLE_PORT_OPEN: u16 = 6;

pub(crate) const VIRTIO_CONSOLE_PORT_NAME: u16 = 7;

pub(crate) const CONTROL_LEN: usize = 8;

pub(crate) const MAX_CONTROL_MESSAGE_LEN: usize = CONTROL_LEN + AGENT_PORT_NAME.len();

// The most one guest TX chain may carry. vioser hands each WriteFile over as
// one chain. Agents since 2026-11-07 write at most 32 KiB at a time, but older
// ones write a whole reply line at once: a 48 KiB guest clipboard is a
// 65,542-byte CLIP line, which the old 64 KiB bound dropped, so the agent channel
// waited for that reply forever. 16 MiB carries the largest guest clipboard the
// host takes (GUEST_CLIP_MAX_BYTES, 8 MiB as base64) and still bounds a
// guest-controlled descriptor length.
pub(crate) const MAX_AGENT_MESSAGE_LEN: usize = 16 * 1024 * 1024;
