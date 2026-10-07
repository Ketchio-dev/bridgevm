//! Queued host-to-guest control messages and their bounded backlog.

use super::*;

/// Control messages held while the guest has no control-RX buffer posted.
/// Bring-up queues at most a handful (two DEVICE_ADDs, PORT_NAME, PORT_OPEN
/// and one re-assert pair), so this only limits a misbehaving guest.
pub(crate) const MAX_PENDING_CONTROL_MESSAGES: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PendingControlMessage {
    pub(crate) len: usize,
    pub(crate) bytes: [u8; MAX_CONTROL_MESSAGE_LEN],
}

impl VirtioConsole {
    pub(crate) fn enqueue_control(&mut self, message: impl Into<PendingControlMessage>) {
        let message = message.into();
        if console_trace_enabled() {
            if let Some(control) = Control::parse(message.as_slice()) {
                eprintln!(
                    "[vcon] ctrl->guest id={} event={} value={} bytes={}",
                    control.id,
                    control.event,
                    control.value,
                    message.len()
                );
            }
        }
        // The guest acknowledges nothing until it posts control-RX buffers, so
        // without a bound a DEVICE_READY loop grows this queue without limit.
        // QEMU drops a control message when no buffer is posted; we keep a
        // small backlog for the ordinary bring-up race and drop beyond it.
        if self.pending_control.len() < MAX_PENDING_CONTROL_MESSAGES {
            self.pending_control.push_back(message);
        }
    }
}

impl PendingControlMessage {
    pub(crate) fn from_slice(bytes: &[u8]) -> Self {
        assert!(bytes.len() <= MAX_CONTROL_MESSAGE_LEN);
        let mut out = [0u8; MAX_CONTROL_MESSAGE_LEN];
        out[..bytes.len()].copy_from_slice(bytes);
        Self {
            len: bytes.len(),
            bytes: out,
        }
    }

    pub(crate) fn agent_port_name() -> Self {
        let mut out = [0u8; MAX_CONTROL_MESSAGE_LEN];
        out[..CONTROL_LEN]
            .copy_from_slice(&Control::new(AGENT_PORT_ID, VIRTIO_CONSOLE_PORT_NAME, 0).bytes());
        out[CONTROL_LEN..MAX_CONTROL_MESSAGE_LEN].copy_from_slice(AGENT_PORT_NAME);
        Self {
            len: MAX_CONTROL_MESSAGE_LEN,
            bytes: out,
        }
    }

    pub(crate) fn as_slice(&self) -> &[u8] {
        &self.bytes[..self.len]
    }

    pub(crate) fn len(&self) -> usize {
        self.len
    }
}

impl From<Control> for PendingControlMessage {
    fn from(control: Control) -> Self {
        Self::from_slice(&control.bytes())
    }
}
