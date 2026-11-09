//! The guest-to-host agent byte buffer and its backpressure bound.

use super::*;

/// Undrained guest-to-host agent bytes at which agent TX stops consuming
/// descriptors. Like a throttled QEMU virtio-serial port, the chains stay in
/// the ring and are consumed on a later poll once the host drains.
pub const MAX_HOST_INBOUND_LEN: usize = 4 * 1024 * 1024;

/// Called (from whichever vCPU thread took the notify) once new guest agent
/// bytes are buffered, so the owner services them now instead of at its next
/// periodic tick. Set at most once per process; unset means "poll".
pub static AGENT_INBOUND_HOOK: std::sync::OnceLock<Box<dyn Fn() + Send + Sync>> =
    std::sync::OnceLock::new();

impl VirtioConsole {
    pub(crate) fn notify_agent_inbound(&self) {
        if !self.host_inbound.is_empty() {
            if let Some(hook) = AGENT_INBOUND_HOOK.get() {
                hook();
            }
        }
    }

    pub fn take_inbound(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.host_inbound)
    }

    pub fn drain_inbound_into(&mut self, out: &mut Vec<u8>) {
        out.append(&mut self.host_inbound);
    }

    pub(crate) fn host_inbound_has_room(&self) -> bool {
        self.host_inbound.len() < MAX_HOST_INBOUND_LEN
    }
}
