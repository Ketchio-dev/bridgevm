//! The guest-to-host agent byte buffer and its backpressure bound.

use super::*;

/// Undrained guest-to-host agent bytes at which agent TX stops consuming
/// descriptors. Like a throttled QEMU virtio-serial port, the chains stay in
/// the ring and are consumed on a later poll once the host drains.
pub(crate) const MAX_HOST_INBOUND_LEN: usize = 4 * 1024 * 1024;

impl VirtioConsole {
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
