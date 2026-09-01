//! Console TX descriptor validation and bounded gathering into reusable scratch.

use super::*;
use crate::fwcfg::GuestMemoryMut;
use crate::guest_memory::append_guest_bytes_bounded;

impl VirtioConsole {
    pub(crate) fn read_chain_into(
        mem: &dyn GuestMemoryMut,
        queue: &VirtioConsoleQueue,
        head: u16,
        descs: &mut Vec<Descriptor>,
        out: &mut Vec<u8>,
        max_len: usize,
    ) -> bool {
        out.clear();
        if !Self::descriptor_chain_into(mem, queue, head, descs) {
            return false;
        }
        if descs.iter().any(|desc| desc.flags & DESC_F_WRITE != 0) {
            return false;
        }
        for desc in descs.iter() {
            if !append_guest_bytes_bounded(mem, desc.addr, desc.len as usize, max_len, out) {
                out.clear();
                return false;
            }
        }
        true
    }
}
