//! TX request-chain validation and bounded gathering into reusable packet scratch.

use super::*;
use crate::fwcfg::GuestMemoryMut;
use crate::guest_memory::append_guest_bytes_bounded;

impl<B: NetBackend> VirtioNet<B> {
    pub(crate) fn tx_frame_from_chain_into(
        mem: &dyn GuestMemoryMut,
        queue: &VirtioNetQueue,
        head: u16,
        descs: &mut Vec<Descriptor>,
        packet: &mut Vec<u8>,
    ) -> bool {
        packet.clear();
        if !Self::descriptor_chain_into(mem, queue, head, descs) {
            return false;
        }
        if descs.iter().any(|desc| desc.flags & DESC_F_WRITE != 0) {
            return false;
        }
        let gathered = descs.iter().all(|desc| {
            append_guest_bytes_bounded(mem, desc.addr, desc.len as usize, MAX_TX_PACKET_LEN, packet)
        });
        let valid = gathered && packet.len() >= VIRTIO_NET_HDR_LEN;
        if !valid {
            packet.clear();
        }
        valid
    }
}
